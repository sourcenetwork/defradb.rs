use super::tests::FakeTransport;
use crate::{DbTransportDocPusher, TransportDocPusher};
use p2p::transport::PeerId;
use query::DocMutator;
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

#[derive(Clone, Copy)]
enum PushPath {
    Replay,
    Document,
    Collection,
}

async fn assert_hint_prevents_resend(path: PushPath) {
    let db = Arc::new(db::DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
    db.create_collection(
        schema::CollectionVersion::new(
            "Users",
            "version-users",
            "collection-users",
            vec![
                schema::FieldDescription::new("1", "_docID", schema::FieldKind::doc_id()),
                schema::FieldDescription::new("2", "name", schema::FieldKind::string()),
            ],
        )
        .as_branchable(),
    )
    .await
    .unwrap();
    let created = db::AutoCommitMutator::new(db.clone())
        .create(
            "Users",
            document::Document::from_json_str(r#"{"name":"Alice"}"#).unwrap(),
        )
        .await
        .unwrap();
    let peer = PeerId::new("remote-peer".into());
    let transport = FakeTransport::with_hint(peer.clone());
    let blockstore = Arc::new(blockstore::DefraBlockstore::new(db.store().clone(), true));
    let (coordinator, _events) = p2p::sync::SyncCoordinator::new(
        transport.clone(),
        blockstore,
        p2p::sync::SyncConfig::default(),
    )
    .await
    .unwrap();
    let pusher = DbTransportDocPusher::new(
        db.clone(),
        transport.clone(),
        coordinator.head_hint_car_authority(),
    );
    pusher
        .persist_replicator(peer.as_str(), &["collection-users".into()])
        .await
        .unwrap();
    let peerstore = storage::stores::Peerstore::new(db.store().clone());
    assert!(peerstore
        .get_retry_info(peer.as_str())
        .await
        .unwrap()
        .is_none());

    for attempt in 0..2 {
        match path {
            PushPath::Replay => {
                // Initial replay hands failures to the durable sweep rather than returning them.
                pusher
                    .push_existing_docs(&peer, &["Users".into()], &Default::default(), None, None)
                    .await
                    .unwrap();
            }
            PushPath::Document => {
                pusher
                    .retry_doc(&peer, &created.doc_id.to_string(), "collection-users")
                    .await
                    .unwrap_err();
            }
            PushPath::Collection => {
                pusher
                    .retry_collection_commit(&peer, "collection-users")
                    .await
                    .unwrap_err();
            }
        }
        assert_eq!(
            transport.sends.load(Ordering::SeqCst),
            1,
            "attempt {attempt}"
        );
        let remaining = db::merge::push_docs_replay::remaining_retry_after(&peerstore, &peer)
            .await
            .unwrap()
            .expect("receiver hint must be durable");
        assert!(remaining > Duration::from_secs(40), "{remaining:?}");
        peerstore.activate_retry_peer(peer.as_str()).await.unwrap();
    }
}

#[tokio::test]
async fn initial_replay_persists_hint_and_reconnect_cannot_bypass_it() {
    assert_hint_prevents_resend(PushPath::Replay).await;
}

#[tokio::test]
async fn document_retry_persists_hint_and_reconnect_cannot_bypass_it() {
    assert_hint_prevents_resend(PushPath::Document).await;
}

#[tokio::test]
async fn collection_retry_persists_hint_and_reconnect_cannot_bypass_it() {
    assert_hint_prevents_resend(PushPath::Collection).await;
}
