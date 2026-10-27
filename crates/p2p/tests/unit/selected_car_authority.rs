use super::*;
use defra_core::{Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload};

async fn coordinator(
    store: Arc<RegolithStore>,
    transport: NoopTransport,
) -> SyncCoordinator<TestBlockstore, NoopTransport> {
    let blockstore = Arc::new(DefraBlockstore::new(store.clone(), true));
    let (coordinator, _events) = SyncCoordinator::with_access_control_and_serve_gate(
        transport,
        blockstore,
        SyncConfig::default(),
        AccessMode::Controlled,
        Arc::new(ReplicatorRegistry::new()),
        Arc::new(MemoryCollectionStorage::default()),
        Arc::new(crate::replicator::EqOnlyFilterMatcher),
        Arc::new(StaticDataClassifier {
            collection_id: "private".into(),
        }),
        Arc::new(LateBoundServeAcp::new()),
    )
    .await
    .unwrap();
    coordinator
        .head_hint_car_authority()
        .bind_peerstore(storage::stores::Peerstore::new(store));
    coordinator
}

#[tokio::test]
async fn disk_reopen_serves_only_selected_root_descendants_without_subscription_or_acp() {
    let directory = tempfile::tempdir().unwrap();
    let peer = random_peer_id();
    let child = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "body".into(),
            collection_version_id: "v1".into(),
            priority: 1,
            data: b"selected".to_vec(),
        }),
        vec![],
        vec![],
    );
    let child_data = child.to_dag_cbor().unwrap();
    let child_cid = child.generate_cid().unwrap();
    let root = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            collection_version_id: "v1".into(),
            priority: 1,
            status: 1,
        }),
        vec![],
        vec![DAGLink::new("body", child_cid)],
    );
    let root_cid = root.generate_cid().unwrap();
    let unrelated = cid_for(b"not selected");
    {
        let store = Arc::new(RegolithStore::open(directory.path()).unwrap());
        let sender = coordinator(store.clone(), NoopTransport::new()).await;
        sender
            .blockstore()
            .put(&child_cid, &child_data)
            .await
            .unwrap();
        sender
            .blockstore()
            .put(&root_cid, &root.to_dag_cbor().unwrap())
            .await
            .unwrap();
        sender
            .blockstore()
            .put(&unrelated, b"not selected")
            .await
            .unwrap();
        let peers = storage::stores::Peerstore::new(store);
        peers
            .grant_selected_car_root(peer.as_str(), &root_cid.to_bytes())
            .await
            .unwrap();
        assert!(!peers.has_replicator(peer.as_str()).await.unwrap());
        sender.shutdown().await;
    }
    let transport = NoopTransport::new();
    let store = Arc::new(RegolithStore::open(directory.path()).unwrap());
    let sender = coordinator(store.clone(), transport.clone()).await;
    assert!(!sender
        .runtime
        .selective_car_access
        .allows_root(&peer, &root_cid));
    for (requester, root, expected) in [
        (
            peer.clone(),
            root_cid,
            vec![(child_cid, child_data.clone())],
        ),
        (random_peer_id(), root_cid, vec![]),
        (peer.clone(), child_cid, vec![]),
    ] {
        sender
            .handle_transport_event(selective_car_fetch_event(
                requester,
                root,
                vec![child_cid, unrelated],
            ))
            .await
            .unwrap();
        let response = transport.car_responses().last().cloned().unwrap();
        let (_, blocks) = crate::sync::car::decode_car(&response).unwrap();
        assert_eq!(blocks, expected);
    }
    let peers = storage::stores::Peerstore::new(store);
    peers
        .revoke_selected_car_root(peer.as_str(), &root_cid.to_bytes())
        .await
        .unwrap();
    sender
        .handle_transport_event(selective_car_fetch_event(peer, root_cid, vec![child_cid]))
        .await
        .unwrap();
    let response = transport.car_responses().last().cloned().unwrap();
    assert!(crate::sync::car::decode_car(&response)
        .unwrap()
        .1
        .is_empty());
    sender.shutdown().await;
}
