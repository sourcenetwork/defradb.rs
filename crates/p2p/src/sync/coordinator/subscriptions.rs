//! Collection and document subscription management.

use blockstore::Blockstore;
use cid::Cid;
use futures::{stream, StreamExt};
use std::sync::Arc;
use std::time::Duration;

use super::SyncCoordinator;
use crate::error::Result;
use crate::transport::P2PTransport;

const MAX_CONCURRENT_COLLECTION_SUBSCRIPTIONS: usize = 16;
const COLLECTION_SUBSCRIPTION_TIMEOUT: Duration = Duration::from_secs(5);
const COLLECTION_UNSUBSCRIBE_RETRY_MIN: Duration = Duration::from_millis(100);
const COLLECTION_UNSUBSCRIBE_RETRY_MAX: Duration = Duration::from_secs(5);

impl<B: Blockstore + 'static, T: P2PTransport> SyncCoordinator<B, T> {
    /// Subscribe to a collection for sync.
    pub async fn subscribe_collection(&self, collection_id: &str) -> Result<bool> {
        let subscribed = self
            .subscribe_collections(&[collection_id.to_string()])
            .await?;
        Ok(subscribed == 1)
    }

    /// Subscribe to collection topics as one durable, bounded batch.
    ///
    /// The durable set is desired state, matching Go DefraDB: it commits
    /// atomically before transport work begins and can therefore heal on a
    /// later call or restart. The live cache contains only topics whose
    /// transport installation succeeded.
    pub async fn subscribe_collections(&self, collection_ids: &[String]) -> Result<usize> {
        let _mutation = self.subscriptions.mutation.lock().await;

        let mut seen = std::collections::HashSet::new();
        let requested = collection_ids
            .iter()
            .filter(|collection_id| seen.insert((*collection_id).clone()))
            .cloned()
            .collect::<Vec<_>>();
        if requested.is_empty() {
            return Ok(0);
        }

        let desired = self
            .subscriptions
            .collection_store
            .get_all_collections()
            .await?
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        let desired_missing = requested
            .iter()
            .filter(|collection_id| !desired.contains(collection_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !desired_missing.is_empty() {
            self.subscriptions
                .collection_store
                .add_collections(&desired_missing)
                .await?;
        }

        let missing = {
            let subscribed_collections = self.subscriptions.subscribed_collections.read().await;
            requested
                .iter()
                .filter(|collection_id| !subscribed_collections.contains(collection_id.as_str()))
                .cloned()
                .collect::<Vec<_>>()
        };

        if missing.is_empty() {
            return Ok(0);
        }

        let broadcaster = self.runtime.broadcaster.clone();
        let results = stream::iter(missing.iter().cloned())
            .map(move |collection_id| {
                let broadcaster = broadcaster.clone();
                async move {
                    let result = tokio::time::timeout(
                        COLLECTION_SUBSCRIPTION_TIMEOUT,
                        broadcaster.subscribe_collection(&collection_id),
                    )
                    .await
                    .unwrap_or_else(|_| {
                        Err(crate::error::Error::Transport(format!(
                            "timed out subscribing to collection {collection_id}"
                        )))
                    });
                    (collection_id, result)
                }
            })
            .buffer_unordered(MAX_CONCURRENT_COLLECTION_SUBSCRIPTIONS)
            .collect::<Vec<_>>()
            .await;

        let mut installed = Vec::new();
        let mut newly_installed = 0;
        let mut first_error = None;
        for (collection_id, result) in results {
            match result {
                Ok(was_new) => {
                    newly_installed += usize::from(was_new);
                    installed.push(collection_id);
                }
                Err(error) => {
                    tracing::warn!(
                        collection_id = %collection_id,
                        error = %error,
                        "Failed to install durable collection subscription"
                    );
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }

        {
            let mut subscribed_collections =
                self.subscriptions.subscribed_collections.write().await;
            subscribed_collections.extend(installed);
        }

        tracing::debug!(
            requested = collection_ids.len(),
            desired_added = desired_missing.len(),
            transport_installed = newly_installed,
            "Applied durable collection subscription batch"
        );
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(newly_installed)
    }

    /// Subscribe to a specific document for sync.
    pub async fn subscribe_document(&self, doc_id: &str) -> Result<bool> {
        self.runtime.broadcaster.subscribe_document(doc_id).await
    }

    /// Unsubscribe from a collection.
    pub async fn unsubscribe_collection(&self, collection_id: &str) -> Result<bool> {
        let removed = self
            .unsubscribe_collections(&[collection_id.to_string()])
            .await?;
        Ok(removed == 1)
    }

    /// Unsubscribe from collection topics as one durable, bounded batch.
    pub async fn unsubscribe_collections(&self, collection_ids: &[String]) -> Result<usize> {
        let _mutation = self.subscriptions.mutation.lock().await;

        let mut seen = std::collections::HashSet::new();
        let requested = collection_ids
            .iter()
            .filter(|collection_id| seen.insert((*collection_id).clone()))
            .cloned()
            .collect::<Vec<_>>();
        if requested.is_empty() {
            return Ok(0);
        }

        // Match Go DefraDB's commit-first semantics: all durable removals
        // succeed atomically before any transport topic is touched.
        self.subscriptions
            .collection_store
            .remove_collections(&requested)
            .await?;

        let live = self.subscriptions.subscribed_collections.read().await;
        let installed = requested
            .iter()
            .filter(|collection_id| live.contains(collection_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        drop(live);

        let broadcaster = self.runtime.broadcaster.clone();
        let results = stream::iter(installed)
            .map(move |collection_id| {
                let broadcaster = broadcaster.clone();
                async move {
                    let result = tokio::time::timeout(
                        COLLECTION_SUBSCRIPTION_TIMEOUT,
                        broadcaster.unsubscribe_collection(&collection_id),
                    )
                    .await
                    .unwrap_or_else(|_| {
                        Err(crate::error::Error::Transport(format!(
                            "timed out unsubscribing from collection {collection_id}"
                        )))
                    });
                    (collection_id, result)
                }
            })
            .buffer_unordered(MAX_CONCURRENT_COLLECTION_SUBSCRIPTIONS)
            .collect::<Vec<_>>()
            .await;

        let mut removed = Vec::new();
        let mut newly_removed = 0;
        let mut failures = Vec::new();
        for (collection_id, result) in results {
            match result {
                Ok(was_removed) => {
                    newly_removed += usize::from(was_removed);
                    removed.push(collection_id);
                }
                Err(error) => failures.push((collection_id, error)),
            }
        }

        let mut live = self.subscriptions.subscribed_collections.write().await;
        for collection_id in removed {
            live.remove(&collection_id);
        }
        drop(live);

        for (collection_id, _) in &failures {
            self.schedule_collection_unsubscribe_retry(collection_id.clone())
                .await;
        }

        if let Some((collection_id, error)) = failures.into_iter().next() {
            tracing::warn!(
                collection_id = %collection_id,
                error = %error,
                "Failed to remove durable collection subscription from live transport; retry scheduled"
            );
            return Err(error);
        }

        tracing::debug!(
            requested = requested.len(),
            transport_removed = newly_removed,
            "Removed durable collection subscription batch"
        );
        Ok(newly_removed)
    }

    /// Unsubscribe from a document.
    pub async fn unsubscribe_document(&self, doc_id: &str) -> Result<bool> {
        self.runtime.broadcaster.unsubscribe_document(doc_id).await
    }

    /// Get the list of subscribed collection IDs.
    pub async fn get_subscribed_collections(&self) -> Result<Vec<String>> {
        let mut collections = self
            .subscriptions
            .collection_store
            .get_all_collections()
            .await?;
        collections.sort();
        Ok(collections)
    }

    async fn schedule_collection_unsubscribe_retry(&self, collection_id: String) {
        let mut retrying = self.subscriptions.retrying_unsubscribes.lock().await;
        if !retrying.insert(collection_id.clone()) {
            return;
        }
        drop(retrying);

        let broadcaster = self.runtime.broadcaster.clone();
        let mutation = Arc::clone(&self.subscriptions.mutation);
        let collection_store = Arc::clone(&self.subscriptions.collection_store);
        let subscribed = Arc::clone(&self.subscriptions.subscribed_collections);
        let retrying = Arc::clone(&self.subscriptions.retrying_unsubscribes);
        let shutdown = self.runtime.shutdown.clone();
        let task_shutdown = shutdown.clone();
        let retry_id = collection_id.clone();
        let spawned = shutdown.spawn_task(async move {
            let mut delay = COLLECTION_UNSUBSCRIBE_RETRY_MIN;
            loop {
                tokio::select! {
                    _ = task_shutdown.cancelled() => break,
                    _ = tokio::time::sleep(delay) => {}
                }

                // Serialize the durable check and transport cleanup with
                // foreground subscription mutations. Either a re-subscribe
                // wins first and cancels cleanup, or cleanup wins and the
                // following re-subscribe reinstalls the topic.
                let _mutation = mutation.lock().await;
                match collection_store.is_subscribed(&retry_id).await {
                    Ok(true) => {
                        retrying.lock().await.remove(&retry_id);
                        return;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(
                            collection_id = %retry_id,
                            error = %error,
                            "Failed to read durable subscription state before unsubscribe retry"
                        );
                        delay = (delay * 2).min(COLLECTION_UNSUBSCRIBE_RETRY_MAX);
                        continue;
                    }
                }

                let result = tokio::time::timeout(
                    COLLECTION_SUBSCRIPTION_TIMEOUT,
                    broadcaster.unsubscribe_collection(&retry_id),
                )
                .await;
                match result {
                    Ok(Ok(_)) => {
                        subscribed.write().await.remove(&retry_id);
                        retrying.lock().await.remove(&retry_id);
                        return;
                    }
                    Ok(Err(error)) => tracing::warn!(
                        collection_id = %retry_id,
                        error = %error,
                        "Collection unsubscribe retry failed"
                    ),
                    Err(_) => tracing::warn!(
                        collection_id = %retry_id,
                        "Collection unsubscribe retry timed out"
                    ),
                }
                delay = (delay * 2).min(COLLECTION_UNSUBSCRIBE_RETRY_MAX);
            }
            retrying.lock().await.remove(&retry_id);
        });

        if !spawned {
            self.subscriptions
                .retrying_unsubscribes
                .lock()
                .await
                .remove(&collection_id);
        }
    }

    /// Load and subscribe to all persisted P2P collections.
    ///
    /// This can run before pubsub_rpc services start: collection topic
    /// subscription is a transport-level operation independent of the base
    /// doc-sync / sync-branchable service topics.
    pub async fn load_p2p_collections(&self) -> Result<usize> {
        let _mutation = self.subscriptions.mutation.lock().await;
        let collections = self
            .subscriptions
            .collection_store
            .get_all_collections()
            .await?;
        let count = collections.len();

        if count == 0 {
            tracing::debug!("No persisted P2P collections to load");
            return Ok(0);
        }

        tracing::info!(count = count, "Loading persisted P2P collections");

        let broadcaster = self.runtime.broadcaster.clone();
        let results = stream::iter(collections)
            .map(move |collection_id| {
                let broadcaster = broadcaster.clone();
                async move {
                    let result = tokio::time::timeout(
                        COLLECTION_SUBSCRIPTION_TIMEOUT,
                        broadcaster.subscribe_collection(&collection_id),
                    )
                    .await
                    .unwrap_or_else(|_| {
                        Err(crate::error::Error::Transport(format!(
                            "timed out restoring collection {collection_id}"
                        )))
                    });
                    (collection_id, result)
                }
            })
            .buffer_unordered(MAX_CONCURRENT_COLLECTION_SUBSCRIPTIONS)
            .collect::<Vec<_>>()
            .await;

        let mut installed = Vec::new();
        for (collection_id, result) in results {
            match result {
                Ok(_) => installed.push(collection_id),
                Err(error) => {
                    tracing::warn!(
                        collection_id = %collection_id,
                        error = %error,
                        "Failed to subscribe to persisted P2P collection"
                    );
                }
            }
        }
        let loaded = installed.len();
        self.subscriptions
            .subscribed_collections
            .write()
            .await
            .extend(installed);

        tracing::info!(loaded = loaded, "Finished loading P2P collections");
        Ok(loaded)
    }

    /// Mark a block as merged.
    pub async fn mark_as_merged(&self, cid: &Cid) -> Result<()> {
        self.manager.mark_as_merged(cid).await
    }

    /// Mark multiple blocks as merged in a single transaction.
    pub async fn mark_batch_as_merged(&self, cids: &[Cid]) -> Result<()> {
        self.manager.mark_batch_as_merged(cids).await
    }

    /// Retire stale receiver state for a root whose merged bit was already
    /// durable before this event was observed.
    pub async fn reconcile_merged_pending(&self, cid: &Cid) -> Result<bool> {
        self.manager.reconcile_merged_pending(cid).await
    }

    /// Quarantine a terminally-rejected pending-DAG root (#1128).
    pub async fn quarantine_pending_dag(&self, root_cid: &Cid, reason: &str) {
        self.manager.quarantine_pending_dag(root_cid, reason).await
    }

    /// Check if a block is merged.
    pub async fn is_merged(&self, cid: &Cid) -> Result<bool> {
        self.manager.is_merged(cid).await
    }

    /// Get all unmerged block CIDs.
    pub async fn get_unmerged(&self) -> Result<Vec<Cid>> {
        self.manager.get_unmerged().await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use async_trait::async_trait;
    use blockstore::DefraBlockstore;
    use cid::Cid;
    use storage::RegolithStore;

    use crate::bitswap::AccessMode;
    use crate::message::{
        BranchableSyncReply, BranchableSyncRequest, DocSyncReply, DocSyncRequest, PushLogBroadcast,
        PushLogReply, PushLogRequest, PushSEArtifactsRequest, QuerySEArtifactsReply,
        QuerySEArtifactsRequest,
    };
    use crate::sync::collection_store::{P2PCollectionStorage, P2PCollectionStore};
    use crate::sync::manager::SyncConfig;
    use crate::topics::DefraTopic;
    use crate::transport::{MessageId, P2PTransport, PeerAddr, PeerId};
    use crate::{QueryId, ReplicatorInfo};

    use super::{SyncCoordinator, MAX_CONCURRENT_COLLECTION_SUBSCRIPTIONS};

    type TestBlockstore = DefraBlockstore<RegolithStore>;

    #[derive(Clone)]
    struct RecordingTransport {
        peer_id: PeerId,
        pubkey: Vec<u8>,
        subscribed: Arc<Mutex<HashSet<String>>>,
        fail_subscribe: Arc<Mutex<HashSet<String>>>,
        fail_unsubscribe_remaining: Arc<Mutex<HashMap<String, usize>>>,
        unsubscribe_started: Option<Arc<tokio::sync::Notify>>,
        unsubscribe_release: Option<Arc<tokio::sync::Notify>>,
        subscribe_calls: Arc<AtomicUsize>,
        subscribe_delay: Duration,
        subscribe_in_flight: Arc<AtomicUsize>,
        max_subscribe_in_flight: Arc<AtomicUsize>,
        replicators: Arc<Mutex<HashMap<String, Vec<String>>>>,
    }

    impl RecordingTransport {
        fn new(peer_id: &str) -> Self {
            Self {
                peer_id: PeerId::new(peer_id.to_string()),
                pubkey: vec![1, 2, 3],
                subscribed: Arc::new(Mutex::new(HashSet::new())),
                fail_subscribe: Arc::new(Mutex::new(HashSet::new())),
                fail_unsubscribe_remaining: Arc::new(Mutex::new(HashMap::new())),
                unsubscribe_started: None,
                unsubscribe_release: None,
                subscribe_calls: Arc::new(AtomicUsize::new(0)),
                subscribe_delay: Duration::ZERO,
                subscribe_in_flight: Arc::new(AtomicUsize::new(0)),
                max_subscribe_in_flight: Arc::new(AtomicUsize::new(0)),
                replicators: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        fn fail_subscribe(self, topic: &str) -> Self {
            self.fail_subscribe
                .lock()
                .unwrap()
                .insert(topic.to_string());
            self
        }

        fn with_subscribe_delay(mut self, delay: Duration) -> Self {
            self.subscribe_delay = delay;
            self
        }

        fn fail_unsubscribe_once(self, topic: &str) -> Self {
            self.fail_unsubscribe_times(topic, 1)
        }

        fn fail_unsubscribe_times(self, topic: &str, times: usize) -> Self {
            self.fail_unsubscribe_remaining
                .lock()
                .unwrap()
                .insert(topic.to_string(), times);
            self
        }

        fn with_unsubscribe_gate(
            mut self,
            started: Arc<tokio::sync::Notify>,
            release: Arc<tokio::sync::Notify>,
        ) -> Self {
            self.unsubscribe_started = Some(started);
            self.unsubscribe_release = Some(release);
            self
        }

        fn subscribe_calls(&self) -> usize {
            self.subscribe_calls.load(Ordering::Relaxed)
        }

        fn max_subscribe_in_flight(&self) -> usize {
            self.max_subscribe_in_flight.load(Ordering::Relaxed)
        }

        fn subscribed_topics(&self) -> Vec<String> {
            let mut topics: Vec<_> = self.subscribed.lock().unwrap().iter().cloned().collect();
            topics.sort();
            topics
        }
    }

    #[async_trait]
    impl P2PTransport for RecordingTransport {
        type ResponseToken = ();

        fn local_peer_id(&self) -> &PeerId {
            &self.peer_id
        }

        fn local_public_key_proto(&self) -> &[u8] {
            &self.pubkey
        }

        fn sign(&self, _data: &[u8]) -> crate::Result<Vec<u8>> {
            Ok(vec![0])
        }

        async fn dial(&self, _peer_id: &PeerId, _addrs: Vec<PeerAddr>) -> crate::Result<()> {
            Ok(())
        }

        async fn disconnect(&self, _peer_id: &PeerId) -> crate::Result<()> {
            Ok(())
        }

        async fn listen(&self, _addr: PeerAddr) -> crate::Result<()> {
            Ok(())
        }

        async fn connected_peers(&self) -> crate::Result<Vec<PeerId>> {
            Ok(Vec::new())
        }

        async fn listen_addresses(&self) -> crate::Result<Vec<PeerAddr>> {
            Ok(Vec::new())
        }

        async fn poll_until_connected(
            &self,
            _peer_id: &PeerId,
            _timeout: Duration,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn peer_addresses(&self) -> crate::Result<Vec<String>> {
            Ok(Vec::new())
        }

        async fn subscribe(&self, topic: DefraTopic) -> crate::Result<bool> {
            let topic = topic.topic_string();
            self.subscribe_calls.fetch_add(1, Ordering::Relaxed);
            let in_flight = self.subscribe_in_flight.fetch_add(1, Ordering::Relaxed) + 1;
            self.max_subscribe_in_flight
                .fetch_max(in_flight, Ordering::Relaxed);
            if !self.subscribe_delay.is_zero() {
                tokio::time::sleep(self.subscribe_delay).await;
            }

            let result = if self.fail_subscribe.lock().unwrap().contains(&topic) {
                Err(crate::error::Error::Transport(format!(
                    "injected subscribe failure for {topic}"
                )))
            } else {
                Ok(self.subscribed.lock().unwrap().insert(topic))
            };
            self.subscribe_in_flight.fetch_sub(1, Ordering::Relaxed);
            result
        }

        async fn unsubscribe(&self, topic: DefraTopic) -> crate::Result<bool> {
            let topic = topic.topic_string();
            let should_fail = {
                let mut remaining = self.fail_unsubscribe_remaining.lock().unwrap();
                match remaining.get_mut(&topic) {
                    Some(count) if *count > 0 => {
                        *count -= 1;
                        true
                    }
                    _ => false,
                }
            };
            if should_fail {
                return Err(crate::error::Error::Transport(format!(
                    "injected unsubscribe failure for {topic}"
                )));
            }
            if let Some(started) = &self.unsubscribe_started {
                started.notify_one();
            }
            if let Some(release) = &self.unsubscribe_release {
                release.notified().await;
            }
            Ok(self.subscribed.lock().unwrap().remove(&topic))
        }

        async fn publish(
            &self,
            _topic: DefraTopic,
            _msg: PushLogBroadcast,
        ) -> crate::Result<MessageId> {
            Ok(MessageId::new("message".to_string()))
        }

        async fn topic_peers(&self, _topic: DefraTopic) -> crate::Result<Vec<PeerId>> {
            Ok(Vec::new())
        }

        async fn send_pushlog_response(
            &self,
            _token: Self::ResponseToken,
            _reply: PushLogReply,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn send_two_stream_request(
            &self,
            _peer_id: &PeerId,
            _req: PushLogRequest,
        ) -> crate::Result<PushLogReply> {
            Ok(PushLogReply::success("ok"))
        }

        async fn send_two_stream_response(
            &self,
            _peer_id: &PeerId,
            _reply: PushLogReply,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn send_doc_sync_request(
            &self,
            _peer_id: &PeerId,
            _req: DocSyncRequest,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn send_doc_sync_response(
            &self,
            _peer_id: &PeerId,
            _reply: DocSyncReply,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn send_branchable_sync_request(
            &self,
            _peer_id: &PeerId,
            _req: BranchableSyncRequest,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn send_branchable_sync_response(
            &self,
            _peer_id: &PeerId,
            _reply: BranchableSyncReply,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn send_car_request(&self, _peer_id: &PeerId, _root_cid: Cid) -> crate::Result<()> {
            Ok(())
        }

        async fn send_car_response(
            &self,
            _peer_id: &PeerId,
            _car_data: Vec<u8>,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn send_car_response_token(
            &self,
            _token: Self::ResponseToken,
            _car_data: Vec<u8>,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn send_doc_sync_response_token(
            &self,
            _token: Self::ResponseToken,
            _reply: DocSyncReply,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn send_branchable_sync_response_token(
            &self,
            _token: Self::ResponseToken,
            _reply: BranchableSyncReply,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn send_se_artifacts(
            &self,
            _peer_id: &PeerId,
            _req: PushSEArtifactsRequest,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn send_se_query_request(
            &self,
            _peer_id: &PeerId,
            _req: QuerySEArtifactsRequest,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn send_se_query_response(
            &self,
            _peer_id: &PeerId,
            _reply: QuerySEArtifactsReply,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn sync_blocks(
            &self,
            _root: Cid,
            _providers: Vec<PeerId>,
            _missing: Vec<Cid>,
        ) -> crate::Result<QueryId> {
            Ok(QueryId(1))
        }

        async fn cancel_sync(&self, _query_id: QueryId) -> crate::Result<bool> {
            Ok(true)
        }

        async fn create_replicator(
            &self,
            peer_id: &PeerId,
            collections: Vec<String>,
        ) -> crate::Result<()> {
            self.replicators
                .lock()
                .unwrap()
                .insert(peer_id.to_string(), collections);
            Ok(())
        }

        async fn delete_replicator(&self, peer_id: &PeerId) -> crate::Result<()> {
            self.replicators.lock().unwrap().remove(peer_id.as_str());
            Ok(())
        }

        async fn list_replicators(&self) -> crate::Result<Vec<ReplicatorInfo>> {
            Ok(Vec::new())
        }

        async fn get_replicator(&self, _peer_id: &PeerId) -> crate::Result<Option<ReplicatorInfo>> {
            Ok(None)
        }

        async fn remove_replicator_collections(
            &self,
            _peer_id: &PeerId,
            _collections: Vec<String>,
        ) -> crate::Result<bool> {
            Ok(false)
        }

        async fn shutdown(&self) -> crate::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingCollectionStore {
        collections: Mutex<HashSet<String>>,
        add_batches: AtomicUsize,
        remove_batches: AtomicUsize,
    }

    impl RecordingCollectionStore {
        fn add_batches(&self) -> usize {
            self.add_batches.load(Ordering::Relaxed)
        }

        fn collections(&self) -> HashSet<String> {
            self.collections.lock().unwrap().clone()
        }

        fn remove_batches(&self) -> usize {
            self.remove_batches.load(Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl P2PCollectionStorage for RecordingCollectionStore {
        async fn add_collection(&self, collection_id: &str) -> crate::Result<()> {
            self.add_batches.fetch_add(1, Ordering::Relaxed);
            self.collections
                .lock()
                .unwrap()
                .insert(collection_id.to_string());
            Ok(())
        }

        async fn add_collections(&self, collection_ids: &[String]) -> crate::Result<()> {
            self.add_batches.fetch_add(1, Ordering::Relaxed);
            self.collections
                .lock()
                .unwrap()
                .extend(collection_ids.iter().cloned());
            Ok(())
        }

        async fn remove_collection(&self, collection_id: &str) -> crate::Result<()> {
            self.remove_collections(&[collection_id.to_string()]).await
        }

        async fn remove_collections(&self, collection_ids: &[String]) -> crate::Result<()> {
            self.remove_batches.fetch_add(1, Ordering::Relaxed);
            let mut collections = self.collections.lock().unwrap();
            for collection_id in collection_ids {
                collections.remove(collection_id);
            }
            Ok(())
        }

        async fn get_all_collections(&self) -> crate::Result<Vec<String>> {
            Ok(self.collections.lock().unwrap().iter().cloned().collect())
        }

        async fn is_subscribed(&self, collection_id: &str) -> crate::Result<bool> {
            Ok(self.collections.lock().unwrap().contains(collection_id))
        }
    }

    async fn new_test_coordinator(
        store: Arc<RegolithStore>,
        transport: RecordingTransport,
    ) -> SyncCoordinator<TestBlockstore, RecordingTransport> {
        let blockstore = Arc::new(DefraBlockstore::new(store.clone(), true));
        let collection_store = Arc::new(P2PCollectionStore::new(store));
        new_test_coordinator_with_store(blockstore, transport, collection_store).await
    }

    async fn new_test_coordinator_with_store(
        blockstore: Arc<TestBlockstore>,
        transport: RecordingTransport,
        collection_store: Arc<dyn P2PCollectionStorage>,
    ) -> SyncCoordinator<TestBlockstore, RecordingTransport> {
        let (coordinator, _events) = SyncCoordinator::with_collection_store(
            transport,
            blockstore,
            SyncConfig::default(),
            AccessMode::Controlled,
            collection_store,
        )
        .await
        .unwrap();
        coordinator
    }

    #[tokio::test]
    async fn subscribe_collections_is_bounded_batched_and_idempotent() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let blockstore = Arc::new(DefraBlockstore::new(store, true));
        let transport =
            RecordingTransport::new("batch-peer").with_subscribe_delay(Duration::from_millis(5));
        let collection_store = Arc::new(RecordingCollectionStore::default());
        let coordinator = new_test_coordinator_with_store(
            blockstore,
            transport.clone(),
            collection_store.clone(),
        )
        .await;
        let collections = (0..70)
            .map(|index| format!("collection-{index}"))
            .collect::<Vec<_>>();

        let started = Instant::now();
        let added = coordinator
            .subscribe_collections(&collections)
            .await
            .unwrap();
        let elapsed = started.elapsed();

        assert_eq!(added, 70);
        assert_eq!(transport.subscribe_calls(), 70);
        assert_eq!(collection_store.add_batches(), 1);
        assert_eq!(collection_store.collections().len(), 70);
        assert!(
            transport.max_subscribe_in_flight() > 1,
            "representative bulk install should overlap transport subscriptions"
        );
        assert!(
            transport.max_subscribe_in_flight() <= MAX_CONCURRENT_COLLECTION_SUBSCRIPTIONS,
            "bulk install must respect its concurrency bound"
        );
        assert!(
            elapsed < Duration::from_millis(250),
            "70 delayed subscriptions should complete as a bounded batch, elapsed={elapsed:?}"
        );

        let added_again = coordinator
            .subscribe_collections(&collections)
            .await
            .unwrap();
        assert_eq!(added_again, 0);
        assert_eq!(transport.subscribe_calls(), 70);
        assert_eq!(collection_store.add_batches(), 1);
    }

    #[tokio::test]
    async fn subscribe_collections_persists_desired_set_and_restart_heals_partial_failure() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let blockstore = Arc::new(DefraBlockstore::new(store, true));
        let transport =
            RecordingTransport::new("partial-failure-peer").fail_subscribe("collection-1");
        let collection_store = Arc::new(RecordingCollectionStore::default());
        let coordinator = new_test_coordinator_with_store(
            blockstore,
            transport.clone(),
            collection_store.clone(),
        )
        .await;
        let collections = (0..3)
            .map(|index| format!("collection-{index}"))
            .collect::<Vec<_>>();

        let error = coordinator
            .subscribe_collections(&collections)
            .await
            .expect_err("one failed transport subscription must fail the batch");

        assert!(error.to_string().contains("injected subscribe failure"));
        assert_eq!(
            transport.subscribed_topics(),
            vec!["collection-0", "collection-2"]
        );
        assert_eq!(collection_store.collections().len(), 3);
        assert_eq!(collection_store.add_batches(), 1);
        assert_eq!(
            coordinator
                .get_subscribed_collections()
                .await
                .unwrap()
                .into_iter()
                .collect::<HashSet<_>>(),
            collections.iter().cloned().collect(),
            "list reports durable desired state even while one live install needs healing"
        );

        let restarted_blockstore = Arc::new(DefraBlockstore::new(
            Arc::new(RegolithStore::in_memory().unwrap()),
            true,
        ));
        let restarted_transport = RecordingTransport::new("restarted-peer");
        let restarted = new_test_coordinator_with_store(
            restarted_blockstore,
            restarted_transport.clone(),
            collection_store,
        )
        .await;

        assert_eq!(restarted.load_p2p_collections().await.unwrap(), 3);
        assert_eq!(restarted_transport.subscribed_topics().len(), 3);
    }

    #[tokio::test]
    async fn load_p2p_collections_reinstalls_subscriptions_after_restart() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let initial_transport = RecordingTransport::new("initial-peer");
        let initial = new_test_coordinator(store.clone(), initial_transport.clone()).await;

        assert!(initial.subscribe_collection("users").await.unwrap());
        assert_eq!(initial_transport.subscribed_topics(), vec!["users"]);

        let restarted_transport = RecordingTransport::new("restarted-peer");
        let restarted = new_test_coordinator(store, restarted_transport.clone()).await;
        assert_eq!(
            restarted.get_subscribed_collections().await.unwrap(),
            vec!["users".to_string()],
            "a fresh coordinator reports durable desired state before live restoration"
        );

        let restored = restarted.load_p2p_collections().await.unwrap();

        assert_eq!(restored, 1);
        assert_eq!(restarted_transport.subscribed_topics(), vec!["users"]);
        assert_eq!(
            restarted.get_subscribed_collections().await.unwrap(),
            vec!["users".to_string()]
        );
    }

    #[tokio::test]
    async fn subscribe_collection_keeps_durable_intent_when_transport_subscribe_fails() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let failing_transport = RecordingTransport::new("failing-peer").fail_subscribe("users");
        let failing = new_test_coordinator(store.clone(), failing_transport.clone()).await;

        let error = failing
            .subscribe_collection("users")
            .await
            .expect_err("transport subscribe failure should be returned");

        assert!(
            error.to_string().contains("injected subscribe failure"),
            "unexpected error: {error}"
        );
        assert_eq!(
            failing.get_subscribed_collections().await.unwrap(),
            vec!["users".to_string()],
            "failed live installation must remain durable desired state"
        );
        assert!(
            failing_transport.subscribed_topics().is_empty(),
            "failing transport should not record a subscription"
        );

        let restarted_transport = RecordingTransport::new("restarted-peer");
        let restarted = new_test_coordinator(store, restarted_transport.clone()).await;
        let restored = restarted.load_p2p_collections().await.unwrap();

        assert_eq!(restored, 1, "restart should retry durable desired state");
        assert_eq!(restarted_transport.subscribed_topics(), vec!["users"]);
    }

    #[tokio::test]
    async fn unsubscribe_failure_keeps_live_topic_retryable() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let blockstore = Arc::new(DefraBlockstore::new(store, true));
        let transport = RecordingTransport::new("unsubscribe-peer").fail_unsubscribe_once("users");
        let collection_store = Arc::new(RecordingCollectionStore::default());
        let coordinator = new_test_coordinator_with_store(
            blockstore,
            transport.clone(),
            collection_store.clone(),
        )
        .await;

        assert!(coordinator.subscribe_collection("users").await.unwrap());
        let error = coordinator
            .unsubscribe_collection("users")
            .await
            .expect_err("injected transport failure should be returned");

        assert!(error.to_string().contains("injected unsubscribe failure"));
        assert!(
            collection_store.collections().is_empty(),
            "durable desired state must record the unsubscribe before transport work"
        );
        assert_eq!(transport.subscribed_topics(), vec!["users"]);
        assert!(coordinator
            .get_subscribed_collections()
            .await
            .unwrap()
            .is_empty());
        assert!(
            coordinator
                .subscriptions
                .subscribed_collections
                .read()
                .await
                .contains("users"),
            "failed live removal must remain cached for retry"
        );

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if transport.subscribed_topics().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("background unsubscribe retry should converge");
        assert!(transport.subscribed_topics().is_empty());
        assert!(
            coordinator
                .subscriptions
                .subscribed_collections
                .read()
                .await
                .is_empty(),
            "successful background retry must retire the live topic"
        );
        assert_eq!(collection_store.remove_batches(), 1);
    }

    #[tokio::test]
    async fn resubscribe_after_failed_unsubscribe_restores_durable_intent() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let blockstore = Arc::new(DefraBlockstore::new(store, true));
        let transport = RecordingTransport::new("resubscribe-peer").fail_unsubscribe_once("users");
        let collection_store = Arc::new(RecordingCollectionStore::default());
        let coordinator = new_test_coordinator_with_store(
            blockstore,
            transport.clone(),
            collection_store.clone(),
        )
        .await;

        assert!(coordinator.subscribe_collection("users").await.unwrap());
        coordinator
            .unsubscribe_collection("users")
            .await
            .expect_err("injected transport failure should be returned");
        assert!(
            !coordinator.subscribe_collection("users").await.unwrap(),
            "already-live topic does not need a second transport install"
        );

        assert_eq!(collection_store.add_batches(), 2);
        assert_eq!(collection_store.remove_batches(), 1);
        assert_eq!(
            coordinator.get_subscribed_collections().await.unwrap(),
            vec!["users".to_string()]
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            transport.subscribed_topics(),
            vec!["users"],
            "unsubscribe retry must stop when the topic becomes desired again"
        );

        let restarted_blockstore = Arc::new(DefraBlockstore::new(
            Arc::new(RegolithStore::in_memory().unwrap()),
            true,
        ));
        let restarted_transport = RecordingTransport::new("restarted-peer");
        let restarted = new_test_coordinator_with_store(
            restarted_blockstore,
            restarted_transport.clone(),
            collection_store,
        )
        .await;
        assert_eq!(restarted.load_p2p_collections().await.unwrap(), 1);
        assert_eq!(restarted_transport.subscribed_topics(), vec!["users"]);
    }

    #[tokio::test]
    async fn resubscribe_serializes_with_in_flight_unsubscribe_retry() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let blockstore = Arc::new(DefraBlockstore::new(store, true));
        let retry_started = Arc::new(tokio::sync::Notify::new());
        let retry_release = Arc::new(tokio::sync::Notify::new());
        let transport = RecordingTransport::new("resubscribe-race-peer")
            .fail_unsubscribe_once("users")
            .with_unsubscribe_gate(Arc::clone(&retry_started), Arc::clone(&retry_release));
        let collection_store = Arc::new(RecordingCollectionStore::default());
        let coordinator = Arc::new(
            new_test_coordinator_with_store(
                blockstore,
                transport.clone(),
                collection_store.clone(),
            )
            .await,
        );

        assert!(coordinator.subscribe_collection("users").await.unwrap());
        coordinator
            .unsubscribe_collection("users")
            .await
            .expect_err("injected transport failure should be returned");

        retry_started.notified().await;
        let retrying_coordinator = Arc::clone(&coordinator);
        let mut resubscribe =
            tokio::spawn(async move { retrying_coordinator.subscribe_collection("users").await });
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut resubscribe)
                .await
                .is_err(),
            "foreground re-subscribe must wait for in-flight cleanup"
        );

        retry_release.notify_one();
        assert!(resubscribe.await.unwrap().unwrap());
        assert_eq!(
            coordinator.get_subscribed_collections().await.unwrap(),
            vec!["users".to_string()]
        );
        assert_eq!(transport.subscribed_topics(), vec!["users"]);
        assert_eq!(collection_store.add_batches(), 2);
        assert_eq!(collection_store.remove_batches(), 1);
    }

    #[tokio::test]
    async fn terminated_retry_releases_ownership_for_a_later_unsubscribe() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let blockstore = Arc::new(DefraBlockstore::new(store, true));
        let transport =
            RecordingTransport::new("retry-handoff-peer").fail_unsubscribe_times("users", 2);
        let collection_store = Arc::new(RecordingCollectionStore::default());
        let coordinator = new_test_coordinator_with_store(
            blockstore,
            transport.clone(),
            collection_store.clone(),
        )
        .await;

        assert!(coordinator.subscribe_collection("users").await.unwrap());
        coordinator
            .unsubscribe_collection("users")
            .await
            .expect_err("first unsubscribe should fail");
        assert!(!coordinator.subscribe_collection("users").await.unwrap());

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if coordinator
                    .subscriptions
                    .retrying_unsubscribes
                    .lock()
                    .await
                    .is_empty()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("retry owner should retire after observing restored intent");

        coordinator
            .unsubscribe_collection("users")
            .await
            .expect_err("second unsubscribe should exercise a fresh retry owner");
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if transport.subscribed_topics().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("fresh retry owner should remove the undesired live topic");

        assert!(coordinator
            .get_subscribed_collections()
            .await
            .unwrap()
            .is_empty());
        assert_eq!(collection_store.add_batches(), 2);
        assert_eq!(collection_store.remove_batches(), 2);
    }

    #[tokio::test]
    async fn unsubscribe_collections_commits_one_durable_batch_before_live_cleanup() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let blockstore = Arc::new(DefraBlockstore::new(store, true));
        let transport =
            RecordingTransport::new("remove-batch-peer").fail_unsubscribe_once("collection-1");
        let collection_store = Arc::new(RecordingCollectionStore::default());
        let coordinator = new_test_coordinator_with_store(
            blockstore,
            transport.clone(),
            collection_store.clone(),
        )
        .await;
        let collections = (0..3)
            .map(|index| format!("collection-{index}"))
            .collect::<Vec<_>>();
        coordinator
            .subscribe_collections(&collections)
            .await
            .unwrap();

        coordinator
            .unsubscribe_collections(&collections)
            .await
            .expect_err("one failed live removal should report the transport error");

        assert!(collection_store.collections().is_empty());
        assert_eq!(collection_store.remove_batches(), 1);
        assert!(coordinator
            .get_subscribed_collections()
            .await
            .unwrap()
            .is_empty());
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if transport.subscribed_topics().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("failed live removal should converge in the background");
    }
}
