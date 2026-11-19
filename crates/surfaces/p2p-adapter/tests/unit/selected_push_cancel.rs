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

/// A one-worker pool, the edge, a database on that pool and its documents.
type Fixture = (
    workers::Pool,
    edge::Edge,
    Arc<db::DB<storage::RegolithStore>>,
    Vec<(String, String)>,
);

/// A one-worker pool, the edge, a database on that pool and `count` documents
/// in its `Users` collection.
fn fixture(count: usize) -> Fixture {
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
                for index in 0..count {
                    let mut doc = document::Document::new(schema.clone());
                    doc.set("name", format!("user-{index}")).unwrap();
                    let created = mutator.create("Users", doc).await.unwrap();
                    docs.push(("Users".to_owned(), created.doc_id.to_string()));
                }
                docs
            }
        })
        .unwrap();
    (pool, edge, db, docs)
}

#[test]
fn dropping_a_pooled_selected_push_stops_its_worker_scheduling_the_rest() {
    let (pool, edge, db, docs) = fixture(SELECTED);
    let peer = PeerId::new("remote-peer".into());
    let transport = FakeTransport::slow(peer.clone(), Duration::from_millis(20));
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
        use futures::FutureExt as _;
        let selection = pusher.push_existing_docs_by_id(&peer, &docs, &stop).fuse();
        let first_sends = async {
            while sends.load(Ordering::SeqCst) < 8 {
                p2p::rt::time::sleep(Duration::from_millis(1)).await;
            }
        }
        .fuse();
        futures::pin_mut!(selection, first_sends);
        futures::select! {
            result = selection => {
                panic!("the selection finished before the caller gave up: {result:?}")
            }
            () = first_sends => {}
        }
    })
    .unwrap();
    let at_drop = sends.load(Ordering::SeqCst);

    // The pool finishes the worker call it admitted before it stops, and that
    // call joins every task it started, so nothing sends after this.
    pool.shutdown_blocking().unwrap();
    let total = sends.load(Ordering::SeqCst);
    assert!(
        total <= at_drop + db::merge::ReplayPushConfig::default().max_concurrent_outbound_pushes,
        "{total} PushLogs after the caller gave up at {at_drop}"
    );
    assert!(total < SELECTED, "the abandoned selection was sent in full");
}

/// A pooled call that returns has a finished worker call behind it, so its
/// drop guard is disarmed: the stop stays lowered and can be used again.
#[test]
fn a_finished_pooled_selected_push_leaves_its_stop_lowered() {
    let (pool, edge, db, docs) = fixture(3);
    let peer = PeerId::new("remote-peer".into());
    let pusher = pooled(
        DbTransportDocPusher::new_arc(
            db.clone(),
            FakeTransport::slow(peer.clone(), Duration::from_millis(1)),
            p2p::sync::HeadHintCarAuthority::default(),
        ),
        Some(&pool),
    );
    let stop = db::merge::SelectedPushStop::new();
    edge.block_on(async {
        for _ in 0..2 {
            pusher
                .push_existing_docs_by_id(&peer, &docs, &stop)
                .await
                .unwrap();
            assert!(!stop.is_stopped(), "{:?}", stop.cause());
        }
    })
    .unwrap();
    pool.shutdown_blocking().unwrap();
}
