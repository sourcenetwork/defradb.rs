use kovan_map::HopscotchMap;
use rapidhash::RapidHashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
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
type TopicSet = HopscotchMap<String, (), rapidhash::fast::RandomState>;
type TopicCounters = HopscotchMap<String, Arc<AtomicUsize>, rapidhash::fast::RandomState>;

fn rapid_map<K, V>() -> HopscotchMap<K, V, rapidhash::fast::RandomState>
where
    K: std::hash::Hash + Eq + Clone + 'static,
    V: Clone + 'static,
{
    HopscotchMap::with_hasher(rapidhash::fast::RandomState::default())
}

/// Consumes one injected failure for `topic`, if any remain.
#[allow(deprecated, reason = "try_update requires Rust 1.95; MSRV is 1.91")]
fn take_failure(remaining: &TopicCounters, topic: &str) -> bool {
    remaining.get(topic).is_some_and(|count| {
        count
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                count.checked_sub(1)
            })
            .is_ok()
    })
}

#[derive(Clone)]
struct RecordingTransport {
    peer_id: PeerId,
    pubkey: Vec<u8>,
    subscribed: Arc<TopicSet>,
    fail_subscribe: Arc<TopicSet>,
    fail_subscribe_remaining: Arc<TopicCounters>,
    fail_unsubscribe_remaining: Arc<TopicCounters>,
    unsubscribe_started: Option<Arc<regolith::sync::Notify>>,
    unsubscribe_release: Option<Arc<regolith::sync::Notify>>,
    subscribe_calls: Arc<AtomicUsize>,
    subscribe_delay: Duration,
    subscribe_in_flight: Arc<AtomicUsize>,
    max_subscribe_in_flight: Arc<AtomicUsize>,
    replicators: Arc<HopscotchMap<String, Vec<String>, rapidhash::fast::RandomState>>,
}

impl RecordingTransport {
    fn new(peer_id: &str) -> Self {
        Self {
            peer_id: PeerId::new(peer_id.to_string()),
            pubkey: vec![1, 2, 3],
            subscribed: Arc::new(rapid_map()),
            fail_subscribe: Arc::new(rapid_map()),
            fail_subscribe_remaining: Arc::new(rapid_map()),
            fail_unsubscribe_remaining: Arc::new(rapid_map()),
            unsubscribe_started: None,
            unsubscribe_release: None,
            subscribe_calls: Arc::new(AtomicUsize::new(0)),
            subscribe_delay: Duration::ZERO,
            subscribe_in_flight: Arc::new(AtomicUsize::new(0)),
            max_subscribe_in_flight: Arc::new(AtomicUsize::new(0)),
            replicators: Arc::new(rapid_map()),
        }
    }

    fn fail_subscribe(self, topic: &str) -> Self {
        self.fail_subscribe.insert(topic.to_string(), ());
        self
    }

    fn fail_subscribe_once(self, topic: &str) -> Self {
        self.fail_subscribe_remaining
            .insert(topic.to_string(), Arc::new(AtomicUsize::new(1)));
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
            .insert(topic.to_string(), Arc::new(AtomicUsize::new(times)));
        self
    }

    fn with_unsubscribe_gate(
        mut self,
        started: Arc<regolith::sync::Notify>,
        release: Arc<regolith::sync::Notify>,
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
        let mut topics: Vec<_> = self.subscribed.keys().collect();
        topics.sort();
        topics
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
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
            crate::rt::time::sleep(self.subscribe_delay).await;
        }

        let should_fail_once = take_failure(&self.fail_subscribe_remaining, &topic);
        let result = if should_fail_once || self.fail_subscribe.contains_key(&topic) {
            Err(crate::error::Error::Transport(format!(
                "injected subscribe failure for {topic}"
            )))
        } else {
            Ok(self.subscribed.insert_if_absent(topic, ()).is_none())
        };
        self.subscribe_in_flight.fetch_sub(1, Ordering::Relaxed);
        result
    }

    async fn unsubscribe(&self, topic: DefraTopic) -> crate::Result<bool> {
        let topic = topic.topic_string();
        let should_fail = take_failure(&self.fail_unsubscribe_remaining, &topic);
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
        Ok(self.subscribed.remove(&topic).is_some())
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

    async fn send_car_response(&self, _peer_id: &PeerId, _car_data: Vec<u8>) -> crate::Result<()> {
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
        _token: Option<Self::ResponseToken>,
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
        self.replicators.insert(peer_id.to_string(), collections);
        Ok(())
    }

    async fn delete_replicator(&self, peer_id: &PeerId) -> crate::Result<()> {
        self.replicators.remove(peer_id.as_str());
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

struct RecordingCollectionStore {
    collections: TopicSet,
    add_batches: AtomicUsize,
    remove_batches: AtomicUsize,
}

impl Default for RecordingCollectionStore {
    fn default() -> Self {
        Self {
            collections: rapid_map(),
            add_batches: AtomicUsize::new(0),
            remove_batches: AtomicUsize::new(0),
        }
    }
}

impl RecordingCollectionStore {
    fn add_batches(&self) -> usize {
        self.add_batches.load(Ordering::Relaxed)
    }

    fn collections(&self) -> RapidHashSet<String> {
        self.collections.keys().collect()
    }

    fn remove_batches(&self) -> usize {
        self.remove_batches.load(Ordering::Relaxed)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl P2PCollectionStorage for RecordingCollectionStore {
    /// Counts a batch only when it writes, as a transaction that finds every
    /// collection subscribed already commits with nothing written.
    async fn add_collections(&self, collection_ids: &[String]) -> crate::Result<usize> {
        let added = collection_ids
            .iter()
            .filter(|collection_id| {
                self.collections
                    .insert_if_absent((*collection_id).clone(), ())
                    .is_none()
            })
            .count();
        if added > 0 {
            self.add_batches.fetch_add(1, Ordering::Relaxed);
        }
        Ok(added)
    }

    async fn remove_collections(&self, collection_ids: &[String]) -> crate::Result<()> {
        self.remove_batches.fetch_add(1, Ordering::Relaxed);
        for collection_id in collection_ids {
            self.collections.remove(collection_id);
        }
        Ok(())
    }

    async fn get_all_collections(&self) -> crate::Result<Vec<String>> {
        Ok(self.collections.keys().collect())
    }

    async fn is_subscribed(&self, collection_id: &str) -> crate::Result<bool> {
        Ok(self.collections.contains_key(collection_id))
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
        crate::network::test_network(),
        SyncConfig::default(),
        AccessMode::Controlled,
        collection_store,
    )
    .await
    .unwrap();
    coordinator
}

/// Whether every collection's reconciler has ended.
fn reconcilers_done(coordinator: &SyncCoordinator<TestBlockstore, RecordingTransport>) -> bool {
    coordinator
        .subscriptions
        .collections
        .shared
        .reconciling
        .is_empty()
}

/// Waits until `done` holds, polling with a bounded backoff, for at most five
/// seconds.
async fn wait_for<F>(mut done: F, expectation: &str)
where
    F: FnMut() -> bool,
{
    let mut delay = Duration::from_millis(1);
    crate::rt::time::timeout(Duration::from_secs(5), async {
        while !done() {
            crate::rt::time::sleep(delay).await;
            delay = (delay * 2).min(Duration::from_millis(50));
        }
    })
    .await
    .expect(expectation);
}

#[nuclei::test]
async fn subscribe_collections_is_bounded_batched_and_idempotent() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let transport =
        RecordingTransport::new("batch-peer").with_subscribe_delay(Duration::from_millis(5));
    let collection_store = Arc::new(RecordingCollectionStore::default());
    let coordinator =
        new_test_coordinator_with_store(blockstore, transport.clone(), collection_store.clone())
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

#[nuclei::test]
async fn subscribe_collections_persists_desired_set_and_restart_heals_partial_failure() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let transport = RecordingTransport::new("partial-failure-peer").fail_subscribe("collection-1");
    let collection_store = Arc::new(RecordingCollectionStore::default());
    let coordinator =
        new_test_coordinator_with_store(blockstore, transport.clone(), collection_store.clone())
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
            .collect::<RapidHashSet<_>>(),
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

#[nuclei::test]
async fn failed_subscribe_retries_durable_intent_without_restart() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let transport = RecordingTransport::new("subscribe-retry-peer").fail_subscribe_once("users");
    let collection_store = Arc::new(RecordingCollectionStore::default());
    let coordinator =
        new_test_coordinator_with_store(blockstore, transport.clone(), collection_store.clone())
            .await;

    coordinator
        .subscribe_collection("users")
        .await
        .expect_err("first live installation should fail after intent is durable");

    assert_eq!(
        coordinator.get_subscribed_collections().await.unwrap(),
        vec!["users".to_string()],
        "the public list is durable desired state"
    );
    assert!(transport.subscribed_topics().is_empty());
    assert_eq!(collection_store.add_batches(), 1);

    wait_for(
        || transport.subscribed_topics() == vec!["users"] && reconcilers_done(&coordinator),
        "background subscribe retry should converge without a restart or caller sweep",
    )
    .await;

    assert!(coordinator.subscriptions.collections.is_live("users"));
    assert!(reconcilers_done(&coordinator));
    assert_eq!(transport.subscribe_calls(), 2);
    assert_eq!(collection_store.add_batches(), 1);
}

#[nuclei::test]
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

#[nuclei::test]
async fn load_p2p_collections_retries_failed_restoration_in_process() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let transport = RecordingTransport::new("restore-retry-peer").fail_subscribe_once("users");
    let collection_store = Arc::new(RecordingCollectionStore::default());
    collection_store
        .add_collections(&["users".to_string()])
        .await
        .unwrap();
    let coordinator =
        new_test_coordinator_with_store(blockstore, transport.clone(), collection_store.clone())
            .await;

    assert_eq!(
        coordinator.load_p2p_collections().await.unwrap(),
        0,
        "the initial restoration reports only topics installed during that call"
    );
    assert!(transport.subscribed_topics().is_empty());

    wait_for(
        || transport.subscribed_topics() == vec!["users"] && reconcilers_done(&coordinator),
        "failed startup restoration should converge in-process",
    )
    .await;

    assert_eq!(transport.subscribe_calls(), 2);
    assert_eq!(collection_store.add_batches(), 1);
    assert!(reconcilers_done(&coordinator));
}

#[nuclei::test]
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

#[nuclei::test]
async fn unsubscribe_failure_keeps_live_topic_retryable() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let transport = RecordingTransport::new("unsubscribe-peer").fail_unsubscribe_once("users");
    let collection_store = Arc::new(RecordingCollectionStore::default());
    let coordinator =
        new_test_coordinator_with_store(blockstore, transport.clone(), collection_store.clone())
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
        coordinator.subscriptions.collections.is_live("users"),
        "failed live removal must remain cached for retry"
    );

    wait_for(
        || transport.subscribed_topics().is_empty() && reconcilers_done(&coordinator),
        "background unsubscribe retry should converge",
    )
    .await;
    assert!(transport.subscribed_topics().is_empty());
    assert!(
        coordinator.subscriptions.collections.shared.live.is_empty(),
        "successful background retry must retire the live topic"
    );
    assert_eq!(collection_store.remove_batches(), 1);
}

#[nuclei::test]
async fn resubscribe_after_failed_unsubscribe_restores_durable_intent() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let transport = RecordingTransport::new("resubscribe-peer").fail_unsubscribe_once("users");
    let collection_store = Arc::new(RecordingCollectionStore::default());
    let coordinator =
        new_test_coordinator_with_store(blockstore, transport.clone(), collection_store.clone())
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
    wait_for(
        || reconcilers_done(&coordinator),
        "the reconciler should end",
    )
    .await;
    assert_eq!(
        transport.subscribed_topics(),
        vec!["users"],
        "the reconciler must keep the topic once it is desired again"
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

/// A resubscribe does not wait for a removal in flight: it commits its
/// intent, finds the topic still live, and returns; the removal then finds
/// the topic desired again, and the reconciler installs it once more.
#[nuclei::test]
async fn resubscribe_during_an_in_flight_removal_returns_at_once_and_converges() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let retry_started = Arc::new(regolith::sync::Notify::new());
    let retry_release = Arc::new(regolith::sync::Notify::new());
    let transport = RecordingTransport::new("resubscribe-race-peer")
        .fail_unsubscribe_once("users")
        .with_unsubscribe_gate(Arc::clone(&retry_started), Arc::clone(&retry_release));
    let collection_store = Arc::new(RecordingCollectionStore::default());
    let coordinator = Arc::new(
        new_test_coordinator_with_store(blockstore, transport.clone(), collection_store.clone())
            .await,
    );

    assert!(coordinator.subscribe_collection("users").await.unwrap());
    coordinator
        .unsubscribe_collection("users")
        .await
        .expect_err("injected transport failure should be returned");

    retry_started.notified().await;
    let resubscribed = crate::rt::time::timeout(
        Duration::from_secs(5),
        coordinator.subscribe_collection("users"),
    )
    .await
    .expect("a resubscribe must not wait for the removal in flight")
    .unwrap();
    assert!(!resubscribed, "the topic was still live when it committed");

    retry_release.notify_one();
    wait_for(
        || transport.subscribed_topics() == vec!["users"] && reconcilers_done(&coordinator),
        "the reconciler should install the topic again",
    )
    .await;
    assert_eq!(
        coordinator.get_subscribed_collections().await.unwrap(),
        vec!["users".to_string()]
    );
    assert!(coordinator.subscriptions.collections.is_live("users"));
    assert_eq!(collection_store.add_batches(), 2);
    assert_eq!(collection_store.remove_batches(), 1);
}

#[nuclei::test]
async fn terminated_retry_releases_ownership_for_a_later_unsubscribe() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let transport =
        RecordingTransport::new("retry-handoff-peer").fail_unsubscribe_times("users", 2);
    let collection_store = Arc::new(RecordingCollectionStore::default());
    let coordinator =
        new_test_coordinator_with_store(blockstore, transport.clone(), collection_store.clone())
            .await;

    assert!(coordinator.subscribe_collection("users").await.unwrap());
    coordinator
        .unsubscribe_collection("users")
        .await
        .expect_err("first unsubscribe should fail");
    assert!(!coordinator.subscribe_collection("users").await.unwrap());

    wait_for(
        || reconcilers_done(&coordinator),
        "the reconciler should end after observing restored intent",
    )
    .await;

    coordinator
        .unsubscribe_collection("users")
        .await
        .expect_err("second unsubscribe should exercise a fresh retry owner");
    wait_for(
        || transport.subscribed_topics().is_empty(),
        "fresh retry owner should remove the undesired live topic",
    )
    .await;

    assert!(coordinator
        .get_subscribed_collections()
        .await
        .unwrap()
        .is_empty());
    assert_eq!(collection_store.add_batches(), 2);
    assert_eq!(collection_store.remove_batches(), 2);
}

#[nuclei::test]
async fn unsubscribe_collections_commits_one_durable_batch_before_live_cleanup() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let transport =
        RecordingTransport::new("remove-batch-peer").fail_unsubscribe_once("collection-1");
    let collection_store = Arc::new(RecordingCollectionStore::default());
    let coordinator =
        new_test_coordinator_with_store(blockstore, transport.clone(), collection_store.clone())
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
    wait_for(
        || transport.subscribed_topics().is_empty(),
        "failed live removal should converge in the background",
    )
    .await;
}
