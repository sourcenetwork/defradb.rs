use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use async_trait::async_trait;
use cid::Cid;
use events::{Bus, ChannelBus, EventName};
use p2p::{transport::PeerId, BitswapStoreAdapter};

use super::tests::{connect_hosts, identity_test_adapter, NoopBlockstore};
use crate::{P2PError, P2POperations, P2PResult, ReplicationFilters, TransportDocPusher};

#[derive(Default)]
struct ReplayPusher {
    calls: Mutex<Vec<Vec<String>>>,
    fail: AtomicBool,
    gate: Mutex<Option<Arc<tokio::sync::Semaphore>>>,
    started: tokio::sync::Notify,
}

#[async_trait]
impl TransportDocPusher for ReplayPusher {
    async fn push_existing_docs(
        &self,
        _: &PeerId,
        collections: &[String],
        _: &p2p::ReplicationFilters,
        _: Option<&[u8]>,
        _: Option<&[u8]>,
    ) -> P2PResult<()> {
        self.calls.lock().unwrap().push(collections.to_vec());
        let gate = self.gate.lock().unwrap().clone();
        if let Some(gate) = gate {
            self.started.notify_one();
            gate.acquire().await.unwrap().forget();
        }
        if self.fail.load(Ordering::Relaxed) {
            Err(P2PError::Internal("replay failed".into()))
        } else {
            Ok(())
        }
    }
    fn get_collection_id(&self, name: &str) -> Option<String> {
        Some(format!("cid-{name}"))
    }
    fn get_collection_name(&self, _: &str) -> P2PResult<Option<String>> {
        unreachable!()
    }
    fn list_collections(&self) -> P2PResult<Vec<String>> {
        Ok(vec![])
    }
    async fn persist_replicator(&self, _: &str, _: &[String]) -> P2PResult<()> {
        Ok(())
    }
    async fn retry_doc(&self, _: &PeerId, _: &str, _: &str) -> P2PResult<()> {
        unreachable!()
    }
    async fn retry_collection_commit(&self, _: &PeerId, _: &str) -> P2PResult<()> {
        unreachable!()
    }
    async fn load_document_head_blocks(&self, _: &str) -> P2PResult<Vec<(Cid, bytes::Bytes)>> {
        unreachable!()
    }
    async fn load_doc_creator_did(&self, _: &str, _: &str) -> P2PResult<Option<String>> {
        unreachable!()
    }
    async fn delete_persisted_replicator(&self, _: &str) -> P2PResult<()> {
        unreachable!()
    }
    async fn persist_p2p_documents(&self, _: &[String]) -> P2PResult<()> {
        unreachable!()
    }
    async fn load_p2p_documents(&self) -> P2PResult<Vec<String>> {
        unreachable!()
    }
    async fn persist_p2p_collections(&self, _: &[String]) -> P2PResult<()> {
        unreachable!()
    }
    fn validate_collection_exists(&self, _: &str) -> P2PResult<()> {
        Ok(())
    }
    fn validate_branchable_collection(&self, _: &str) -> P2PResult<()> {
        unreachable!()
    }
}

#[tokio::test]
async fn every_successful_install_emits_a_correlated_completion() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let (host, handle, _events, _) =
            p2p::host::P2PHost::new(BitswapStoreAdapter::new(Arc::new(NoopBlockstore)))
                .await
                .unwrap();
        let (remote, remote_handle, _remote_events, _) =
            p2p::host::P2PHost::new(BitswapStoreAdapter::new(Arc::new(NoopBlockstore)))
                .await
                .unwrap();
        let local_task = tokio::spawn(host.run());
        let remote_task = tokio::spawn(remote.run());
        connect_hosts(&handle, &remote_handle).await;
        let peer = remote_handle.local_peer_id_cached();
        let address = format!(
            "{}/p2p/{peer}",
            remote_handle.listen_addresses().await.unwrap()[0]
        );
        let bus = Arc::new(ChannelBus::default());
        let mut sub = bus.subscribe(&[EventName::ReplicatorCompleted]);
        let pusher = Arc::new(ReplayPusher::default());
        let mut adapter = identity_test_adapter(handle);
        adapter.event_bus = Some(bus);
        adapter.doc_pusher = Some(pusher.clone());
        for (collections, skipped, fails) in [
            (vec![], true, false),
            (vec!["Note"], false, false),
            (vec!["Note"], true, false),
            (vec!["Note", "User"], false, false),
            (vec!["User"], true, false),
            (vec!["User", "Failed"], false, true),
        ] {
            pusher.fail.store(fails, Ordering::Relaxed);
            let requested: Vec<String> = collections.into_iter().map(str::to_owned).collect();
            adapter
                .add_replicator(
                    requested.clone(),
                    Some(&address),
                    ReplicationFilters::new(),
                    vec![],
                    None,
                )
                .await
                .unwrap();
            let message = sub.recv().await.unwrap();
            let data = message.as_replicator_completed().expect("completion data");
            assert_eq!(data.peer_id, peer.to_string());
            assert_eq!(data.collections, requested);
            assert_eq!(data.skipped, skipped);
            assert_eq!(
                data.error.as_deref(),
                fails.then_some("internal error: replay failed")
            );
            assert!(sub.try_recv().is_err(), "one completion per install");
        }
        assert_eq!(
            *pusher.calls.lock().unwrap(),
            vec![vec!["Note"], vec!["User"], vec!["Failed"]]
        );

        pusher.fail.store(false, Ordering::Relaxed);
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        *pusher.gate.lock().unwrap() = Some(gate.clone());
        let requested = vec!["User".into(), "Failed".into(), "Pending".into()];
        adapter.add_replicator(requested.clone(), Some(&address), ReplicationFilters::new(), vec![], None).await.unwrap();
        pusher.started.notified().await;
        let repeated = adapter.add_replicator(requested, Some(&address), ReplicationFilters::new(), vec![], None);
        tokio::pin!(repeated);
        tokio::select! {
            result = &mut repeated => panic!("install completed before the earlier replay: {result:?}"),
            message = sub.recv() => panic!("premature completion: {message:?}"),
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
        gate.add_permits(1);
        let message = sub.recv().await.unwrap();
        assert!(!message.as_replicator_completed().unwrap().skipped);
        repeated.await.unwrap();
        let message = sub.recv().await.unwrap();
        assert!(message.as_replicator_completed().unwrap().skipped);
        assert!(sub.try_recv().is_err());
        local_task.abort();
        remote_task.abort();
    })
    .await
    .expect("each installation must complete");
}
