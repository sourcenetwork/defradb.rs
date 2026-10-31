//! Dropping a selected push made through the pool stops the worker call it
//! started, which runs on once started (`SelectedPushIsolation.tla` Cancel).

use super::tests::FakeTransport;
use crate::{pooled, DbTransportDocPusher};
use p2p::transport::PeerId;
use query::DocMutator;
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

const SELECTED: usize = 200;

#[test]
fn dropping_a_pooled_selected_push_stops_its_worker_scheduling_the_rest() {
    let pool = storage::start_pool(workers::Config::unplaced(1), None).unwrap();
    let edge = edge::testing::edge();
    let store = Arc::new(
        storage::RegolithStore::open_with_options(
            "regolith-memory",
            storage::RegolithStoreOptions::memory().placed_on(&pool),
        )
        .unwrap(),
    );
    let db = Arc::new(db::DB::from_arc(store).unwrap());
    db.set_network(edge.clone());
    let docs = edge
        .block_on({
            let db = db.clone();
            async move {
                db.create_collection(schema::CollectionVersion::new(
                    "Users",
                    "version-users",
                    "collection-users",
                    vec![
                        schema::FieldDescription::new(
                            "1",
                            "_docID",
                            schema::FieldKind::doc_id(),
                            schema::CType::LwwRegister,
                        ),
                        schema::FieldDescription::new(
                            "2",
                            "name",
                            schema::FieldKind::string(),
                            schema::CType::LwwRegister,
                        ),
                    ],
                ))
                .await
                .unwrap();
                let schema = db.get_collection("Users").unwrap().unwrap().schema_arc();
                let mutator = db::AutoCommitMutator::new(db.clone());
                let mut docs = Vec::new();
                for index in 0..SELECTED {
                    let mut doc = document::Document::new(schema.clone());
                    doc.set("name", format!("user-{index}")).unwrap();
                    let created = mutator.create("Users", doc).await.unwrap();
                    docs.push(("Users".to_owned(), created.doc_id.to_string()));
                }
                docs
            }
        })
        .unwrap();
    let peer = PeerId::new("remote-peer".into());
    let reply_delay = Duration::from_millis(20);
    let transport = FakeTransport::slow(peer.clone(), reply_delay);
    let sends = transport.sends.clone();
    let pusher = pooled(
        DbTransportDocPusher::new_arc(
            db.clone(),
            transport,
            p2p::sync::HeadHintCarAuthority::default(),
        ),
        Some(&pool),
    );
    let stop = db::merge::SelectedPushStop::new();

    // The caller gives up once the worker has its first sends on the wire.
    edge.block_on(async {
        tokio::select! {
            result = pusher.push_existing_docs_by_id(&peer, &docs, &stop) => {
                panic!("the selection finished before the caller gave up: {result:?}")
            }
            () = async {
                while sends.load(Ordering::SeqCst) < 8 {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            } => {}
        }
    })
    .unwrap();
    let at_drop = sends.load(Ordering::SeqCst);

    // Long enough for the abandoned worker call to send the whole selection
    // (25 rounds of eight 20 ms replies) had nothing stopped it.
    std::thread::sleep(reply_delay * 25 * 4);
    let total = sends.load(Ordering::SeqCst);
    assert!(
        total <= at_drop + db::merge::ReplayPushConfig::default().max_concurrent_outbound_pushes,
        "{total} PushLogs after the caller gave up at {at_drop}"
    );
    assert!(total < SELECTED, "the abandoned selection was sent in full");
    pool.shutdown_blocking().unwrap();
}
