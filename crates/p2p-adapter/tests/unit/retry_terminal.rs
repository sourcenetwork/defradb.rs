//! A retry marker whose document is gone from this node has a terminal state:
//! the sweep that loads it discharges it instead of failing on it every pass.

use super::tests::FakeTransport;
use crate::{DbTransportDocPusher, TransportDocPusher};
use p2p::transport::PeerId;
use query::DocMutator;
use std::sync::{atomic::Ordering, Arc};

#[tokio::test]
async fn truncated_doc_marker_is_cleared_by_sweep() {
    let db = Arc::new(db::DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
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
    let created = db::AutoCommitMutator::new(db.clone())
        .create(
            "Users",
            document::Document::from_json_str(
                r#"{"name":"Alice"}"#,
                db.get_collection("Users").unwrap().unwrap().schema_arc(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let doc_id = created.doc_id.to_string();
    let peer = PeerId::new("remote-peer".into());
    let transport = FakeTransport::new(Some(vec![peer.clone()]));
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
    peerstore
        .observe_push_head(peer.as_str(), &doc_id, "collection-users")
        .await
        .unwrap();
    db.truncate_collection("Users", None).await.unwrap();

    let pusher: Arc<dyn TransportDocPusher> = Arc::new(pusher);
    crate::retry::run_retry_pass(&peerstore, &transport, &pusher, None, true).await;

    let left: Vec<String> = peerstore
        .get_retry_documents(peer.as_str())
        .await
        .unwrap()
        .into_iter()
        .map(|marker| marker.doc_id)
        .collect();
    assert!(
        left.is_empty(),
        "the marker of a truncated document survived a due sweep: {left:?}"
    );
    assert_eq!(transport.sends.load(Ordering::SeqCst), 0);
}
