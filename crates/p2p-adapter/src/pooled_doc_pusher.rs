//! The boundary between the P2P layer and the worker pool for its storage
//! work: a [`TransportDocPusher`] whose calls run on the database's worker
//! pool. The P2P layer (the persisted retry sweep, a replicator's replay, its
//! persistence) calls it from the network edge; each call crosses to a
//! worker once, reads and writes the store there, and hands its sends back to
//! the edge (`db::DB::network`). Called on a worker, or without a pool (a
//! test, a one-shot tool), a call runs where it is made.

use std::sync::Arc;

use crate::transport_doc_pusher::TransportDocPusher;
#[cfg(not(target_arch = "wasm32"))]
use {
    crate::{P2PError, P2PErrorExt as _, P2PResult},
    async_trait::async_trait,
    bytes::Bytes,
    cid::Cid,
    p2p::transport::PeerId,
    std::future::Future,
};

/// `inner` as the P2P layer holds it: on `pool` when the database has one,
/// as it is otherwise.
pub fn pooled(
    inner: Arc<dyn TransportDocPusher>,
    pool: Option<&workers::Pool>,
) -> Arc<dyn TransportDocPusher> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(pool) = pool {
        return Arc::new(PooledDocPusher {
            inner,
            pool: pool.clone(),
        });
    }
    // A browser's one worker is the page's thread: a call runs where it is
    // made.
    #[cfg(target_arch = "wasm32")]
    let _ = pool;
    inner
}

/// `inner`'s storage calls, run on a pool's workers.
#[cfg(not(target_arch = "wasm32"))]
struct PooledDocPusher {
    inner: Arc<dyn TransportDocPusher>,
    pool: workers::Pool,
}

#[cfg(not(target_arch = "wasm32"))]
impl PooledDocPusher {
    /// Runs `call` on `inner` on the least-loaded worker.
    async fn on_pool<T, F, Fut>(&self, call: F) -> P2PResult<T>
    where
        F: FnOnce(Arc<dyn TransportDocPusher>) -> Fut + Send + 'static,
        Fut: Future<Output = P2PResult<T>> + 'static,
        T: Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        workers::run_on(Some(&self.pool), workers::Target::LeastLoaded, move || {
            call(inner)
        })
        .await
        .map_err(|error| P2PError::internal(error.report("P2P storage call")))?
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl TransportDocPusher for PooledDocPusher {
    async fn prepare_replicator_install(
        &self,
        request: &crate::ReplicatorInstallRequest,
    ) -> P2PResult<crate::ReplicatorInstall> {
        let request = request.clone();
        self.on_pool(move |inner| async move { inner.prepare_replicator_install(&request).await })
            .await
    }

    async fn finish_replicator_install(
        &self,
        plan: &crate::ReplicatorInstall,
        error: Option<String>,
        bus: Option<Arc<dyn events::Bus>>,
    ) -> P2PResult<()> {
        let plan = plan.clone();
        self.on_pool(move |inner| async move {
            inner.finish_replicator_install(&plan, error, bus).await
        })
        .await
    }

    async fn recover_replicator_installs(&self) -> P2PResult<()> {
        self.on_pool(|inner| async move { inner.recover_replicator_installs().await })
            .await
    }

    async fn push_retry_marker_stats(&self) -> P2PResult<storage::stores::PushRetryMarkerStats> {
        self.on_pool(|inner| async move { inner.push_retry_marker_stats().await })
            .await
    }

    async fn push_retries_for_peer(
        &self,
        peer_id: &str,
    ) -> P2PResult<Option<crate::peer_details::PeerPushRetries>> {
        let peer_id = peer_id.to_owned();
        self.on_pool(move |inner| async move { inner.push_retries_for_peer(&peer_id).await })
            .await
    }

    async fn replication_acknowledged(
        &self,
        peer_id: &str,
        collection_name: &str,
        doc_ids: &[String],
    ) -> P2PResult<Vec<bool>> {
        let (peer_id, collection_name, doc_ids) = (
            peer_id.to_owned(),
            collection_name.to_owned(),
            doc_ids.to_vec(),
        );
        self.on_pool(move |inner| async move {
            inner
                .replication_acknowledged(&peer_id, &collection_name, &doc_ids)
                .await
        })
        .await
    }

    async fn document_heads_acknowledged(
        &self,
        peer_id: &str,
        collection_name: &str,
        doc_ids: &[String],
    ) -> P2PResult<Vec<bool>> {
        let (peer_id, collection_name, doc_ids) = (
            peer_id.to_owned(),
            collection_name.to_owned(),
            doc_ids.to_vec(),
        );
        self.on_pool(move |inner| async move {
            inner
                .document_heads_acknowledged(&peer_id, &collection_name, &doc_ids)
                .await
        })
        .await
    }

    async fn push_existing_docs(
        &self,
        peer_id: &PeerId,
        collections: &[String],
        filters: &p2p::ReplicationFilters,
        se_key: Option<&[u8]>,
        se_identity_pubkey: Option<&[u8]>,
    ) -> P2PResult<()> {
        let (peer_id, collections, filters) =
            (peer_id.clone(), collections.to_vec(), filters.clone());
        let (se_key, se_identity_pubkey) = (
            se_key.map(<[u8]>::to_vec),
            se_identity_pubkey.map(<[u8]>::to_vec),
        );
        self.on_pool(move |inner| async move {
            inner
                .push_existing_docs(
                    &peer_id,
                    &collections,
                    &filters,
                    se_key.as_deref(),
                    se_identity_pubkey.as_deref(),
                )
                .await
        })
        .await
    }

    async fn push_existing_docs_by_id(
        &self,
        peer_id: &PeerId,
        docs: &[(String, String)],
    ) -> P2PResult<()> {
        let (peer_id, docs) = (peer_id.clone(), docs.to_vec());
        self.on_pool(
            move |inner| async move { inner.push_existing_docs_by_id(&peer_id, &docs).await },
        )
        .await
    }

    async fn retry_doc(
        &self,
        peer_id: &PeerId,
        doc_id: &str,
        collection_id: &str,
        generation: u64,
    ) -> P2PResult<()> {
        let (peer_id, doc_id, collection_id) =
            (peer_id.clone(), doc_id.to_owned(), collection_id.to_owned());
        self.on_pool(move |inner| async move {
            inner
                .retry_doc(&peer_id, &doc_id, &collection_id, generation)
                .await
        })
        .await
    }

    async fn retry_collection_commit(
        &self,
        peer_id: &PeerId,
        collection_id: &str,
        generation: u64,
    ) -> P2PResult<()> {
        let (peer_id, collection_id) = (peer_id.clone(), collection_id.to_owned());
        self.on_pool(move |inner| async move {
            inner
                .retry_collection_commit(&peer_id, &collection_id, generation)
                .await
        })
        .await
    }

    async fn load_document_head_blocks(&self, doc_id: &str) -> P2PResult<Vec<(Cid, Bytes)>> {
        let doc_id = doc_id.to_owned();
        self.on_pool(move |inner| async move { inner.load_document_head_blocks(&doc_id).await })
            .await
    }

    async fn load_doc_creator_did(
        &self,
        collection_name: &str,
        doc_id: &str,
    ) -> P2PResult<Option<String>> {
        let (collection_name, doc_id) = (collection_name.to_owned(), doc_id.to_owned());
        self.on_pool(move |inner| async move {
            inner.load_doc_creator_did(&collection_name, &doc_id).await
        })
        .await
    }

    fn get_collection_id(&self, name: &str) -> Option<String> {
        self.inner.get_collection_id(name)
    }

    fn get_collection_name(&self, collection_id: &str) -> P2PResult<Option<String>> {
        self.inner.get_collection_name(collection_id)
    }

    fn list_collections(&self) -> P2PResult<Vec<String>> {
        self.inner.list_collections()
    }

    fn validate_replication_filters(&self, filters: &p2p::ReplicationFilters) -> P2PResult<()> {
        self.inner.validate_replication_filters(filters)
    }

    async fn persist_replicator(&self, peer_id: &str, collections: &[String]) -> P2PResult<()> {
        let (peer_id, collections) = (peer_id.to_owned(), collections.to_vec());
        self.on_pool(
            move |inner| async move { inner.persist_replicator(&peer_id, &collections).await },
        )
        .await
    }

    async fn persist_replicator_info(&self, info: &p2p::ReplicatorInfo) -> P2PResult<()> {
        let info = info.clone();
        self.on_pool(move |inner| async move { inner.persist_replicator_info(&info).await })
            .await
    }

    async fn delete_persisted_replicator(&self, peer_id: &str) -> P2PResult<()> {
        let peer_id = peer_id.to_owned();
        self.on_pool(move |inner| async move { inner.delete_persisted_replicator(&peer_id).await })
            .await
    }

    async fn load_persisted_replicators(&self) -> P2PResult<Option<Vec<p2p::ReplicatorInfo>>> {
        self.on_pool(|inner| async move { inner.load_persisted_replicators().await })
            .await
    }

    async fn persist_p2p_documents(&self, doc_ids: &[String]) -> P2PResult<()> {
        let doc_ids = doc_ids.to_vec();
        self.on_pool(move |inner| async move { inner.persist_p2p_documents(&doc_ids).await })
            .await
    }

    async fn load_p2p_documents(&self) -> P2PResult<Vec<String>> {
        self.on_pool(|inner| async move { inner.load_p2p_documents().await })
            .await
    }

    async fn persist_p2p_collections(&self, collections: &[String]) -> P2PResult<()> {
        let collections = collections.to_vec();
        self.on_pool(move |inner| async move { inner.persist_p2p_collections(&collections).await })
            .await
    }

    fn validate_collection_exists(&self, name: &str) -> P2PResult<()> {
        self.inner.validate_collection_exists(name)
    }

    fn validate_branchable_collection(&self, collection_id: &str) -> P2PResult<()> {
        self.inner.validate_branchable_collection(collection_id)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    /// Records whether its last storage call ran on a pool worker.
    #[derive(Default)]
    struct WhereCalled(AtomicBool);

    impl WhereCalled {
        fn note(&self) -> P2PResult<()> {
            self.0.store(workers::on_worker(), Ordering::SeqCst);
            Ok(())
        }
    }

    #[async_trait]
    impl TransportDocPusher for WhereCalled {
        async fn push_existing_docs(
            &self,
            _: &PeerId,
            _: &[String],
            _: &p2p::ReplicationFilters,
            _: Option<&[u8]>,
            _: Option<&[u8]>,
        ) -> P2PResult<()> {
            self.note()
        }
        async fn retry_doc(&self, _: &PeerId, _: &str, _: &str, _: u64) -> P2PResult<()> {
            self.note()
        }
        async fn retry_collection_commit(&self, _: &PeerId, _: &str, _: u64) -> P2PResult<()> {
            self.note()
        }
        async fn load_document_head_blocks(&self, _: &str) -> P2PResult<Vec<(Cid, Bytes)>> {
            self.note().map(|()| Vec::new())
        }
        async fn load_doc_creator_did(&self, _: &str, _: &str) -> P2PResult<Option<String>> {
            self.note().map(|()| None)
        }
        fn get_collection_id(&self, _: &str) -> Option<String> {
            None
        }
        fn get_collection_name(&self, _: &str) -> P2PResult<Option<String>> {
            Ok(None)
        }
        fn list_collections(&self) -> P2PResult<Vec<String>> {
            Ok(Vec::new())
        }
        async fn persist_replicator(&self, _: &str, _: &[String]) -> P2PResult<()> {
            self.note()
        }
        async fn delete_persisted_replicator(&self, _: &str) -> P2PResult<()> {
            self.note()
        }
        async fn persist_p2p_documents(&self, _: &[String]) -> P2PResult<()> {
            self.note()
        }
        async fn load_p2p_documents(&self) -> P2PResult<Vec<String>> {
            self.note().map(|()| Vec::new())
        }
        async fn persist_p2p_collections(&self, _: &[String]) -> P2PResult<()> {
            self.note()
        }
        fn validate_collection_exists(&self, _: &str) -> P2PResult<()> {
            Ok(())
        }
        fn validate_branchable_collection(&self, _: &str) -> P2PResult<()> {
            Ok(())
        }
    }

    #[test]
    fn a_retry_called_off_the_pool_runs_on_a_worker() {
        let pool = workers::Pool::start(workers::Config::unplaced(1)).unwrap();
        let inner = Arc::new(WhereCalled::default());
        let pusher = pooled(inner.clone(), Some(&pool));
        let peer = PeerId::new("peer-1".to_string());
        workers::block_on(async {
            pusher.retry_doc(&peer, "bae-1", "c1", 1).await.unwrap();
        })
        .unwrap();
        assert!(inner.0.load(Ordering::SeqCst), "the retry ran on a worker");
        workers::block_on(async {
            pusher
                .persist_p2p_documents(&["bae-1".to_string()])
                .await
                .unwrap();
        })
        .unwrap();
        assert!(inner.0.load(Ordering::SeqCst), "so does its persistence");
        pool.shutdown_blocking().unwrap();
    }
}
