use std::sync::{atomic::Ordering, Arc};
use std::time::Duration;

use events::{Bus, ChannelBus, EventName};
use p2p::{transport::PeerId, BitswapStoreAdapter};
use regolith::sync::{Notify, Semaphore};

use super::tests::{connect_hosts, identity_test_adapter, NoopBlockstore};
use crate::{P2POperations, ReplicationFilters};

#[cfg(not(target_arch = "wasm32"))]
async fn replay_install_through_pooled_db_pusher(replace: bool) {
    use query::DocMutator;

    p2p::rt::time::timeout(Duration::from_secs(60), async {
        let (host, handle, _events, _) = p2p::host::P2PHost::new(
            BitswapStoreAdapter::new(Arc::new(NoopBlockstore)),
            edge::testing::edge(),
        )
        .await
        .unwrap();
        let (remote, remote_handle, _events, _) = p2p::host::P2PHost::new(
            BitswapStoreAdapter::new(Arc::new(NoopBlockstore)),
            edge::testing::edge(),
        )
        .await
        .unwrap();
        let local_task = p2p::rt::spawn(host.run());
        let remote_task = p2p::rt::spawn(remote.run());
        connect_hosts(&handle, &remote_handle).await;
        let peer = PeerId::new(remote_handle.local_peer_id_cached().to_string());
        let address = format!(
            "{}/p2p/{peer}",
            remote_handle.listen_addresses().await.unwrap()[0]
        );

        let db = Arc::new(db::DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
        db.set_network(edge::testing::edge());
        db.create_collection(
            schema::CollectionVersion::new(
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
            )
            .as_branchable(),
        )
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

        let started = Arc::new(Notify::new());
        let gate = Arc::new(Semaphore::new(0));
        let mut transport = crate::retry::tests::FakeTransport::with_hint(peer.clone());
        transport.connected_gate = Some((started.clone(), gate.clone()));
        let blockstore = Arc::new(blockstore::DefraBlockstore::new(db.store().clone(), true));
        let (coordinator, _events) = p2p::sync::SyncCoordinator::new(
            transport.clone(),
            blockstore,
            p2p::network::Network::on(edge::testing::edge()),
            p2p::sync::SyncConfig::default(),
        )
        .await
        .unwrap();
        let inner = Arc::new(crate::DbTransportDocPusher::new(
            db.clone(),
            transport.clone(),
            coordinator.head_hint_car_authority(),
        ));
        let pool = workers::Pool::start(workers::Config::unplaced(1)).unwrap();
        let pusher = crate::pooled(inner, Some(&pool));
        let bus = Arc::new(ChannelBus::default());
        let mut sub = bus.subscribe(&[EventName::ReplicatorCompleted]);
        let mut adapter = identity_test_adapter(handle);
        adapter.event_bus = Some(bus);
        adapter.doc_pusher = Some(pusher);
        adapter
            .add_replicator(
                vec!["Users".into()],
                Some(&address),
                ReplicationFilters::new(),
                vec![],
                None,
            )
            .await
            .unwrap();
        started.notified().await;
        let peerstore = db.peerstore();
        let original = peerstore
            .get_replicator_incarnation(peer.as_str())
            .await
            .unwrap()
            .unwrap();
        let replacement = if replace {
            let info =
                p2p::ReplicatorInfo::from_raw(peer.to_string(), vec!["replacement".into()], vec![]);
            let bytes = info.to_bytes().unwrap();
            peerstore
                .create_replicator(peer.as_str(), &bytes)
                .await
                .unwrap();
            Some(bytes)
        } else {
            None
        };
        gate.add_permits(1);
        let message = sub.recv().await.unwrap();
        let result = message.as_replicator_completed().unwrap();
        let incarnation = peerstore
            .get_replicator_incarnation(peer.as_str())
            .await
            .unwrap()
            .unwrap();
        if let Some(bytes) = replacement {
            assert_ne!(incarnation, original);
            assert!(result
                .error
                .as_deref()
                .unwrap()
                .contains("changed before existing-document replay"));
            assert_eq!(
                peerstore
                    .get_replicator(peer.as_str())
                    .await
                    .unwrap()
                    .unwrap()
                    .as_ref(),
                bytes.as_slice()
            );
            assert_eq!(transport.sends.load(Ordering::SeqCst), 0);
            assert!(peerstore
                .get_retry_documents(peer.as_str())
                .await
                .unwrap()
                .is_empty());
        } else {
            assert_eq!(incarnation, original);
            assert!(result.error.is_none(), "{result:?}");
            assert_eq!(transport.sends.load(Ordering::SeqCst), 1);
            let retries = peerstore.get_retry_documents(peer.as_str()).await.unwrap();
            assert_eq!(retries.len(), 1);
            assert_eq!(retries[0].doc_id, created.doc_id.to_string());
            let bytes = peerstore
                .get_replicator(peer.as_str())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                p2p::ReplicatorInfo::from_bytes(&bytes).unwrap().status,
                p2p::ReplicatorStatus::Inactive
            );
        }
        local_task.abort();
        remote_task.abort();
        pool.shutdown_blocking().unwrap();
    })
    .await
    .expect("pooled replay install must finish");
}

#[cfg(not(target_arch = "wasm32"))]
#[nuclei::test]
async fn pooled_db_replay_failure_preserves_install_identity() {
    replay_install_through_pooled_db_pusher(false).await;
}

#[cfg(not(target_arch = "wasm32"))]
#[nuclei::test]
async fn pooled_db_replay_rejects_an_install_replaced_before_its_check() {
    replay_install_through_pooled_db_pusher(true).await;
}
