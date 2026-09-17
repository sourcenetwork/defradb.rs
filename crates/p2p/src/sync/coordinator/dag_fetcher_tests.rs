use super::*;

#[path = "../../../tests/unit/car_size_retry.rs"]
mod car_size_retry;
use crate::error::Result as P2PResult;
use crate::message::{
    BranchableSyncReply, BranchableSyncRequest, DocSyncReply, DocSyncRequest, PushLogBroadcast,
    PushLogReply, PushLogRequest, PushSEArtifactsRequest,
};
use crate::sync::manager::SyncEvent;
use crate::topics::DefraTopic;
use crate::transport::{MessageId, P2PTransport, PeerAddr, PeerId};
use crate::{QueryId, ReplicatorInfo};
use async_trait::async_trait;
use blockstore::{Blockstore, DefraBlockstore};
use ipld_core::{codec::Codec, ipld, ipld::Ipld};
use kovan::{Atom, AtomOption};
use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use multihash_codetable::{Code, MultihashDigest};
use rapidhash::{HashMapExt, RapidHashMap, RapidHashSet};
use serde_ipld_dagcbor::codec::DagCborCodec;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use storage::RegolithStore;
use tokio::sync::mpsc;

type StreamedBlocks = Arc<AtomOption<Vec<(Cid, Vec<u8>)>>>;
type CompletionSlot = Arc<AtomOption<crate::sync::manager::BlockSyncCompletionTracker>>;

fn rapid_map<K, V>() -> HopscotchMap<K, V, rapidhash::fast::RandomState>
where
    K: std::hash::Hash + Eq + Clone + 'static,
    V: Clone + 'static,
{
    HopscotchMap::with_hasher(rapidhash::fast::RandomState::default())
}

fn make_cid(data: &[u8]) -> Cid {
    let hash = Code::Sha2_256.digest(data);
    Cid::new_v1(0x71, hash)
}

fn encode_ipld(ipld: Ipld) -> Vec<u8> {
    DagCborCodec::encode_to_vec(&ipld).unwrap()
}

fn diagnostics() -> Arc<SyncDiagnostics> {
    Arc::new(SyncDiagnostics::default())
}

#[derive(Clone)]
struct TestTransport {
    peer_id: PeerId,
    pubkey: Vec<u8>,
    blockstore: Arc<DefraBlockstore<RegolithStore>>,
    root_cid: Cid,
    root_data: Vec<u8>,
    car_blocks: Arc<RapidHashMap<Cid, Vec<u8>>>,
    selective_blocks: Arc<RapidHashMap<Cid, Vec<u8>>>,
    car_requests: Arc<AtomicUsize>,
    sync_batches: Arc<Atom<Vec<Vec<Cid>>>>,
    sync_providers: Arc<SegQueue<String>>,
    dead_providers: Arc<HopscotchMap<String, (), rapidhash::fast::RandomState>>,
    skip_serving_syncs: Arc<AtomicUsize>,
    fail_connected_peers: Arc<AtomicBool>,
    connected_peers: Arc<Atom<Vec<PeerId>>>,
    cancelled_queries: Arc<SegQueue<u64>>,
    hang_car_requests: Arc<AtomicBool>,
    streamed_rooted_blocks: StreamedBlocks,
    stream_completion: CompletionSlot,
    early_failure_completion: CompletionSlot,
    early_deferred_completion: CompletionSlot,
    early_success_completion: CompletionSlot,
    stream_block_delay_nanos: Arc<AtomicU64>,
    stream_completed: Arc<AtomicBool>,
    cancelled_before_stream_complete: Arc<AtomicBool>,
    size_limited_providers: Arc<
        HopscotchMap<
            String,
            (Cid, crate::sync::manager::BlockSyncCompletionTracker),
            rapidhash::fast::RandomState,
        >,
    >,
}

impl TestTransport {
    fn new(
        blockstore: Arc<DefraBlockstore<RegolithStore>>,
        root_cid: Cid,
        root_data: Vec<u8>,
        car_blocks: RapidHashMap<Cid, Vec<u8>>,
        selective_blocks: RapidHashMap<Cid, Vec<u8>>,
    ) -> Self {
        Self {
            peer_id: PeerId::new("local-peer".to_string()),
            pubkey: vec![1, 2, 3],
            blockstore,
            root_cid,
            root_data,
            car_blocks: Arc::new(car_blocks),
            selective_blocks: Arc::new(selective_blocks),
            car_requests: Arc::new(AtomicUsize::new(0)),
            sync_batches: Arc::new(Atom::new(Vec::new())),
            sync_providers: Arc::new(SegQueue::new()),
            dead_providers: Arc::new(rapid_map()),
            skip_serving_syncs: Arc::new(AtomicUsize::new(0)),
            fail_connected_peers: Arc::new(AtomicBool::new(false)),
            connected_peers: Arc::new(Atom::new(vec![
                PeerId::new("remote-peer".to_string()),
                PeerId::new("dead-peer".to_string()),
                PeerId::new("alt-peer".to_string()),
                PeerId::new("other-peer".to_string()),
            ])),
            cancelled_queries: Arc::new(SegQueue::new()),
            hang_car_requests: Arc::new(AtomicBool::new(false)),
            streamed_rooted_blocks: Arc::new(AtomOption::none()),
            stream_completion: Arc::new(AtomOption::none()),
            early_failure_completion: Arc::new(AtomOption::none()),
            early_deferred_completion: Arc::new(AtomOption::none()),
            early_success_completion: Arc::new(AtomOption::none()),
            stream_block_delay_nanos: Arc::new(AtomicU64::new(
                Duration::from_millis(10).as_nanos() as u64,
            )),
            stream_completed: Arc::new(AtomicBool::new(false)),
            cancelled_before_stream_complete: Arc::new(AtomicBool::new(false)),
            size_limited_providers: Arc::new(rapid_map()),
        }
    }

    fn car_request_count(&self) -> usize {
        self.car_requests.load(Ordering::SeqCst)
    }

    fn sync_batches(&self) -> Vec<Vec<Cid>> {
        self.sync_batches.load_clone()
    }

    fn sync_providers(&self) -> Vec<String> {
        std::iter::from_fn(|| self.sync_providers.pop()).collect()
    }

    fn mark_provider_dead(&self, peer: &str) {
        self.dead_providers.insert(peer.to_string(), ());
    }

    fn set_skip_serving_syncs(&self, count: usize) {
        self.skip_serving_syncs.store(count, Ordering::SeqCst);
    }

    fn set_fail_connected_peers(&self) {
        self.fail_connected_peers.store(true, Ordering::SeqCst);
    }

    fn set_connected_peers(&self, peers: Vec<PeerId>) {
        self.connected_peers.store(peers);
    }

    fn cancelled_queries(&self) -> Vec<u64> {
        std::iter::from_fn(|| self.cancelled_queries.pop()).collect()
    }

    fn set_hang_car_requests(&self) {
        self.hang_car_requests.store(true, Ordering::SeqCst);
    }

    fn set_streamed_rooted_blocks(
        &self,
        blocks: Vec<(Cid, Vec<u8>)>,
        completion: crate::sync::manager::BlockSyncCompletionTracker,
    ) {
        self.streamed_rooted_blocks.store_some(blocks);
        self.stream_completion.store_some(completion);
    }

    fn set_early_failure_completion(
        &self,
        completion: crate::sync::manager::BlockSyncCompletionTracker,
    ) {
        self.early_failure_completion.store_some(completion);
    }

    fn set_early_deferred_completion(
        &self,
        completion: crate::sync::manager::BlockSyncCompletionTracker,
    ) {
        self.early_deferred_completion.store_some(completion);
    }

    fn set_early_success_completion(
        &self,
        completion: crate::sync::manager::BlockSyncCompletionTracker,
    ) {
        self.early_success_completion.store_some(completion);
    }

    fn cancelled_before_stream_complete(&self) -> bool {
        self.cancelled_before_stream_complete.load(Ordering::SeqCst)
    }

    fn stream_block_delay(&self) -> Duration {
        Duration::from_nanos(self.stream_block_delay_nanos.load(Ordering::SeqCst))
    }

    fn set_stream_block_delay(&self, delay: Duration) {
        self.stream_block_delay_nanos
            .store(delay.as_nanos() as u64, Ordering::SeqCst);
    }
}

#[async_trait]
impl P2PTransport for TestTransport {
    type ResponseToken = ();

    fn local_peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    fn supports_cancellable_rooted_sync(&self) -> bool {
        self.streamed_rooted_blocks.is_some()
    }

    fn local_public_key_proto(&self) -> &[u8] {
        &self.pubkey
    }

    fn sign(&self, _data: &[u8]) -> P2PResult<Vec<u8>> {
        Ok(vec![0])
    }

    async fn dial(&self, _peer_id: &PeerId, _addrs: Vec<PeerAddr>) -> P2PResult<()> {
        Ok(())
    }

    async fn disconnect(&self, _peer_id: &PeerId) -> P2PResult<()> {
        Ok(())
    }

    async fn listen(&self, _addr: PeerAddr) -> P2PResult<()> {
        Ok(())
    }

    async fn connected_peers(&self) -> P2PResult<Vec<PeerId>> {
        if self.fail_connected_peers.load(Ordering::SeqCst) {
            return Err(crate::error::Error::Transport(
                "peer listing unavailable".to_string(),
            ));
        }
        Ok(self.connected_peers.load_clone())
    }

    async fn listen_addresses(&self) -> P2PResult<Vec<PeerAddr>> {
        Ok(Vec::new())
    }

    async fn poll_until_connected(&self, _peer_id: &PeerId, _timeout: Duration) -> P2PResult<()> {
        Ok(())
    }

    async fn peer_addresses(&self) -> P2PResult<Vec<String>> {
        Ok(Vec::new())
    }

    async fn topic_peers(&self, _topic: DefraTopic) -> P2PResult<Vec<PeerId>> {
        Ok(Vec::new())
    }

    async fn subscribe(&self, _topic: DefraTopic) -> P2PResult<bool> {
        Ok(true)
    }

    async fn unsubscribe(&self, _topic: DefraTopic) -> P2PResult<bool> {
        Ok(true)
    }

    async fn publish(&self, _topic: DefraTopic, _msg: PushLogBroadcast) -> P2PResult<MessageId> {
        Ok(MessageId::new("noop".to_string()))
    }

    async fn send_pushlog_response(
        &self,
        _token: Self::ResponseToken,
        _reply: PushLogReply,
    ) -> P2PResult<()> {
        Ok(())
    }

    async fn send_two_stream_request(
        &self,
        _peer_id: &PeerId,
        _req: PushLogRequest,
    ) -> P2PResult<PushLogReply> {
        Ok(PushLogReply::success("noop"))
    }

    async fn send_two_stream_response(
        &self,
        _peer_id: &PeerId,
        _reply: PushLogReply,
    ) -> P2PResult<()> {
        Ok(())
    }

    async fn send_doc_sync_request(
        &self,
        _peer_id: &PeerId,
        _req: DocSyncRequest,
    ) -> P2PResult<()> {
        Ok(())
    }

    async fn send_doc_sync_response(
        &self,
        _peer_id: &PeerId,
        _reply: DocSyncReply,
    ) -> P2PResult<()> {
        Ok(())
    }

    async fn send_branchable_sync_request(
        &self,
        _peer_id: &PeerId,
        _req: BranchableSyncRequest,
    ) -> P2PResult<()> {
        Ok(())
    }

    async fn send_branchable_sync_response(
        &self,
        _peer_id: &PeerId,
        _reply: BranchableSyncReply,
    ) -> P2PResult<()> {
        Ok(())
    }

    async fn send_car_request(&self, _peer_id: &PeerId, root_cid: Cid) -> P2PResult<()> {
        assert_eq!(root_cid, self.root_cid);
        self.car_requests.fetch_add(1, Ordering::SeqCst);
        if self.hang_car_requests.load(Ordering::SeqCst) {
            n0_future::time::sleep(Duration::from_secs(600)).await;
            return Ok(());
        }
        self.blockstore
            .put(&self.root_cid, &self.root_data)
            .await
            .map_err(|e| crate::error::Error::BlockstoreError(e.to_string()))?;
        for (cid, data) in self.car_blocks.iter() {
            self.blockstore
                .put(cid, data)
                .await
                .map_err(|e| crate::error::Error::BlockstoreError(e.to_string()))?;
        }
        Ok(())
    }

    async fn send_car_response(&self, _peer_id: &PeerId, _car_data: Vec<u8>) -> P2PResult<()> {
        Ok(())
    }

    async fn send_car_response_token(
        &self,
        _token: Self::ResponseToken,
        _car_data: Vec<u8>,
    ) -> P2PResult<()> {
        Ok(())
    }

    async fn send_doc_sync_response_token(
        &self,
        _token: Self::ResponseToken,
        _reply: DocSyncReply,
    ) -> P2PResult<()> {
        Ok(())
    }

    async fn send_branchable_sync_response_token(
        &self,
        _token: Self::ResponseToken,
        _reply: BranchableSyncReply,
    ) -> P2PResult<()> {
        Ok(())
    }

    async fn send_se_artifacts(
        &self,
        _peer_id: &PeerId,
        _req: PushSEArtifactsRequest,
    ) -> P2PResult<()> {
        Ok(())
    }

    async fn sync_blocks(
        &self,
        _root: Cid,
        providers: Vec<PeerId>,
        missing: Vec<Cid>,
    ) -> P2PResult<QueryId> {
        let mut call_index = 0;
        self.sync_batches.rcu(|batches| {
            let mut next = batches.clone();
            next.push(missing.clone());
            call_index = next.len() - 1;
            next
        });
        for peer in &providers {
            self.sync_providers.push(peer.to_string());
        }
        let query_id = QueryId(call_index as u64 + 1);
        if let Some((cid, completion)) = providers.first().and_then(|peer| {
            self.size_limited_providers
                .get(peer.as_str())
                .filter(|(cid, _)| missing.contains(cid))
        }) {
            completion.size_limit(query_id, cid);
            return Ok(query_id);
        }
        if let Some(completion) = self.early_failure_completion.load().as_deref().cloned() {
            completion.complete(query_id, false);
            return Ok(query_id);
        }
        if let Some(completion) = self.early_deferred_completion.load().as_deref().cloned() {
            completion.defer(query_id);
            return Ok(query_id);
        }
        if let Some(completion) = self.early_success_completion.load().as_deref().cloned() {
            // Production ordering on libp2p: the host reports success once it
            // has forwarded every block *event*; the coordinator dispatches
            // those events and the completion as independent tasks, so the
            // poll owner can observe Success while the puts are still landing.
            completion.complete(query_id, true);
            let blockstore = Arc::clone(&self.blockstore);
            let delay = self.stream_block_delay();
            let blocks: Vec<(Cid, Vec<u8>)> = missing
                .iter()
                .filter_map(|cid| {
                    self.selective_blocks
                        .get(cid)
                        .map(|data| (*cid, data.clone()))
                })
                .collect();
            n0_future::task::spawn(async move {
                n0_future::time::sleep(delay).await;
                for (cid, data) in blocks {
                    blockstore.put(&cid, &data).await.unwrap();
                }
            });
            return Ok(query_id);
        }
        let streamed_blocks = self
            .streamed_rooted_blocks
            .take()
            .map(|blocks| blocks.to_vec());
        if let Some(streamed_blocks) = streamed_blocks {
            let blockstore = Arc::clone(&self.blockstore);
            let completion = self.stream_completion.load().as_deref().cloned();
            let stream_block_delay = self.stream_block_delay();
            let stream_completed = Arc::clone(&self.stream_completed);
            n0_future::task::spawn(async move {
                for (cid, data) in streamed_blocks {
                    n0_future::time::sleep(stream_block_delay).await;
                    blockstore.put(&cid, &data).await.unwrap();
                }
                stream_completed.store(true, Ordering::SeqCst);
                if let Some(completion) = completion {
                    completion.complete(query_id, true);
                }
            });
            return Ok(query_id);
        }
        if call_index < self.skip_serving_syncs.load(Ordering::SeqCst) {
            return Ok(query_id);
        }
        let all_dead = providers
            .iter()
            .all(|peer| self.dead_providers.contains_key(peer.as_str()));
        if all_dead {
            return Ok(query_id);
        }
        for cid in missing {
            if let Some(data) = self.selective_blocks.get(&cid) {
                self.blockstore
                    .put(&cid, data)
                    .await
                    .map_err(|e| crate::error::Error::BlockstoreError(e.to_string()))?;
            }
        }
        Ok(query_id)
    }

    async fn cancel_sync(&self, query_id: QueryId) -> P2PResult<bool> {
        if self.stream_completion.is_some() && !self.stream_completed.load(Ordering::SeqCst) {
            self.cancelled_before_stream_complete
                .store(true, Ordering::SeqCst);
        }
        self.cancelled_queries.push(query_id.0);
        Ok(true)
    }

    async fn create_replicator(
        &self,
        _peer_id: &PeerId,
        _collections: Vec<String>,
    ) -> P2PResult<()> {
        Ok(())
    }

    async fn delete_replicator(&self, _peer_id: &PeerId) -> P2PResult<()> {
        Ok(())
    }

    async fn list_replicators(&self) -> P2PResult<Vec<ReplicatorInfo>> {
        Ok(Vec::new())
    }

    async fn get_replicator(&self, _peer_id: &PeerId) -> P2PResult<Option<ReplicatorInfo>> {
        Ok(None)
    }

    async fn remove_replicator_collections(
        &self,
        _peer_id: &PeerId,
        _collections: Vec<String>,
    ) -> P2PResult<bool> {
        Ok(false)
    }

    async fn shutdown(&self) -> P2PResult<()> {
        Ok(())
    }
}

#[tokio::test]
async fn poll_fetch_dag_recovers_partial_car_with_batched_selective_fetch() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));

    let child_one_data = encode_ipld(ipld!({ "value": 1 }));
    let child_one_cid = make_cid(&child_one_data);
    let child_two_data = encode_ipld(ipld!({ "value": 2 }));
    let child_two_cid = make_cid(&child_two_data);
    let root_data = encode_ipld(ipld!({ "children": [child_one_cid, child_two_cid] }));
    let root_cid = make_cid(&root_data);

    let selective_blocks = RapidHashMap::from_iter([
        (child_one_cid, child_one_data.clone()),
        (child_two_cid, child_two_data.clone()),
    ]);
    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        selective_blocks,
    );

    let (event_tx, mut event_rx) = mpsc::channel(1);
    let source_peer = PeerId::new("remote-peer".to_string());

    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            source_peer.clone(),
        )
        .with_explicit_replicator(true),
        DagFetchLimiter::new(2),
        diagnostics(),
    )
    .await;

    match event_rx.recv().await {
        Some(SyncEvent::DagReady {
            root_cid: ready_cid,
            doc_id,
            collection_id,
            creator,
            sender_peer,
            is_explicit_replicator,
            ..
        }) => {
            assert_eq!(ready_cid, root_cid);
            assert_eq!(doc_id, "doc-id");
            assert_eq!(collection_id, "collection-id");
            assert_eq!(creator, "creator-id");
            assert_eq!(sender_peer.as_deref(), Some(source_peer.as_str()));
            assert!(is_explicit_replicator);
        }
        other => panic!("expected DagReady, got {:?}", other),
    }
    assert!(matches!(blockstore.has(&child_one_cid).await, Ok(true)));
    assert!(matches!(blockstore.has(&child_two_cid).await, Ok(true)));
    assert_eq!(transport.car_request_count(), 1);

    let batches = transport.sync_batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].len(), 2);

    let requested: RapidHashSet<_> = batches[0].iter().copied().collect();
    assert_eq!(
        requested,
        RapidHashSet::from_iter([child_one_cid, child_two_cid])
    );
}

#[tokio::test]
async fn poll_fetch_dag_uses_known_missing_frontier_without_recursive_car() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));

    let child_data = encode_ipld(ipld!({ "value": 1 }));
    let child_cid = make_cid(&child_data);
    let root_data = encode_ipld(ipld!({ "child": child_cid }));
    let root_cid = make_cid(&root_data);

    blockstore.put(&root_cid, &root_data).await.unwrap();

    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(child_cid, child_data.clone())]),
    );

    let (event_tx, mut event_rx) = mpsc::channel(1);
    let source_peer = PeerId::new("remote-peer".to_string());

    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            source_peer,
        ),
        DagFetchLimiter::new(2),
        diagnostics(),
    )
    .await;

    assert!(matches!(
        event_rx.recv().await,
        Some(SyncEvent::DagReady { root_cid: ready_cid, .. }) if ready_cid == root_cid
    ));
    assert!(matches!(blockstore.has(&child_cid).await, Ok(true)));
    assert_eq!(transport.car_request_count(), 0);
    assert_eq!(transport.sync_batches(), vec![vec![child_cid]]);
}

#[tokio::test(start_paused = true)]
async fn rooted_selective_response_drains_before_query_is_reaped() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));

    let leaf_data = encode_ipld(ipld!({ "value": 1 }));
    let leaf_cid = make_cid(&leaf_data);
    let child_data = encode_ipld(ipld!({ "child": leaf_cid }));
    let child_cid = make_cid(&child_data);
    let root_data = encode_ipld(ipld!({ "child": child_cid }));
    let root_cid = make_cid(&root_data);
    blockstore.put(&root_cid, &root_data).await.unwrap();

    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(leaf_cid, leaf_data)]),
    );
    let completion = crate::sync::manager::BlockSyncCompletionTracker::default();
    transport.set_streamed_rooted_blocks(vec![(child_cid, child_data)], completion.clone());
    // The old 10-second coordinator poll window cancelled this otherwise
    // healthy response before Iroh's 30-second transport bound could report
    // completion. Paused time keeps the regression deterministic and fast.
    transport.set_stream_block_delay(Duration::from_secs(11));

    let (event_tx, mut event_rx) = mpsc::channel(1);
    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            PeerId::new("remote-peer".to_string()),
        )
        .with_block_sync_completions(completion)
        .with_rooted_provider_discovery(),
        DagFetchLimiter::new(2),
        diagnostics(),
    )
    .await;

    assert!(matches!(
        event_rx.recv().await,
        Some(SyncEvent::DagReady { root_cid: ready_cid, .. }) if ready_cid == root_cid
    ));
    assert!(matches!(blockstore.has(&leaf_cid).await, Ok(true)));
    assert!(
        !transport.cancelled_before_stream_complete(),
        "a productive rooted selective CAR must drain before cancellation"
    );
    assert_eq!(
        transport.sync_batches(),
        vec![vec![child_cid], vec![leaf_cid]]
    );
}

#[tokio::test(start_paused = true)]
async fn exact_selective_batch_does_not_wait_for_a_lost_completion_signal() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));

    let child_data = encode_ipld(ipld!({ "value": 1 }));
    let child_cid = make_cid(&child_data);
    let root_data = encode_ipld(ipld!({ "child": child_cid }));
    let root_cid = make_cid(&root_data);
    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(child_cid, child_data)]),
    );
    let completion = crate::sync::manager::BlockSyncCompletionTracker::default();
    let context = DagFetchContext::new(
        "doc-id".to_string(),
        "collection-id".to_string(),
        "creator-id".to_string(),
        PeerId::new("remote-peer".to_string()),
    )
    .with_block_sync_completions(completion);
    let started = n0_future::time::Instant::now();

    let outcome = poll_fetch_blocks(
        &root_cid,
        &[child_cid],
        &transport,
        &blockstore,
        &PeerId::new("remote-peer".to_string()),
        &context,
    )
    .await;

    assert_eq!(outcome, ProviderWindowOutcome::Complete);
    assert_eq!(n0_future::time::Instant::now(), started);
    assert!(matches!(blockstore.has(&child_cid).await, Ok(true)));
}

#[tokio::test(start_paused = true)]
async fn exact_selective_failure_before_waiter_registration_is_observed_immediately() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));

    let child_data = encode_ipld(ipld!({ "value": 1 }));
    let child_cid = make_cid(&child_data);
    let root_data = encode_ipld(ipld!({ "child": child_cid }));
    let root_cid = make_cid(&root_data);
    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::new(),
    );
    let completion = crate::sync::manager::BlockSyncCompletionTracker::default();
    transport.set_early_failure_completion(completion.clone());
    let context = DagFetchContext::new(
        "doc-id".to_string(),
        "collection-id".to_string(),
        "creator-id".to_string(),
        PeerId::new("remote-peer".to_string()),
    )
    .with_block_sync_completions(completion);
    let started = n0_future::time::Instant::now();

    let outcome = poll_fetch_blocks(
        &root_cid,
        &[child_cid],
        &transport,
        &blockstore,
        &PeerId::new("remote-peer".to_string()),
        &context,
    )
    .await;

    assert_eq!(outcome, ProviderWindowOutcome::Stalled);
    assert_eq!(
        n0_future::time::Instant::now(),
        started,
        "an early terminal result must not burn the 30-second watchdog"
    );
    assert_eq!(transport.cancelled_queries(), vec![1]);
}

#[tokio::test(start_paused = true)]
async fn contended_car_ingest_defers_to_root_clock_without_fetch_exhaustion() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (root_cid, root_data, child_cid, child_data) = single_child_dag();
    blockstore.put(&root_cid, &root_data).await.unwrap();

    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(child_cid, child_data)]),
    );
    let completion = crate::sync::manager::BlockSyncCompletionTracker::default();
    transport.set_early_deferred_completion(completion.clone());
    let diagnostics = diagnostics();
    let (event_tx, mut event_rx) = mpsc::channel(1);

    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            PeerId::new("remote-peer".to_string()),
        )
        .with_block_sync_completions(completion),
        DagFetchLimiter::new(2),
        diagnostics.clone(),
    )
    .await;

    assert!(event_rx.recv().await.is_none());
    assert_eq!(transport.sync_batches().len(), 1);
    assert_eq!(transport.cancelled_queries(), vec![1]);
    let snapshot = diagnostics.snapshot();
    assert_eq!(snapshot.pending_dag_fetch_deferred_contention, 1);
    assert_eq!(snapshot.pending_dag_fetch_exhausted, 0);
}

#[tokio::test]
async fn poll_fetch_dag_continues_after_partial_selective_batch_progress() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));

    let leaf_one_data = encode_ipld(ipld!({ "value": 1 }));
    let leaf_one_cid = make_cid(&leaf_one_data);
    let leaf_two_data = encode_ipld(ipld!({ "value": 2 }));
    let leaf_two_cid = make_cid(&leaf_two_data);

    let mid_one_data = encode_ipld(ipld!({ "child": leaf_one_cid }));
    let mid_one_cid = make_cid(&mid_one_data);
    let mid_two_data = encode_ipld(ipld!({ "child": leaf_two_cid }));
    let mid_two_cid = make_cid(&mid_two_data);

    let root_data = encode_ipld(ipld!({ "children": [mid_one_cid, mid_two_cid] }));
    let root_cid = make_cid(&root_data);

    let selective_blocks = RapidHashMap::from_iter([
        (mid_one_cid, mid_one_data.clone()),
        (mid_two_cid, mid_two_data.clone()),
        (leaf_one_cid, leaf_one_data.clone()),
        (leaf_two_cid, leaf_two_data.clone()),
    ]);
    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        selective_blocks,
    );

    let (event_tx, mut event_rx) = mpsc::channel(1);
    let source_peer = PeerId::new("remote-peer".to_string());

    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            source_peer,
        ),
        DagFetchLimiter::new(2),
        diagnostics(),
    )
    .await;

    assert!(matches!(
        event_rx.recv().await,
        Some(SyncEvent::DagReady { root_cid: ready_cid, .. }) if ready_cid == root_cid
    ));

    let batches = transport.sync_batches();
    assert_eq!(batches.len(), 2);
    assert_eq!(
        batches[0].iter().copied().collect::<RapidHashSet<_>>(),
        RapidHashSet::from_iter([mid_one_cid, mid_two_cid])
    );
    assert_eq!(
        batches[1].iter().copied().collect::<RapidHashSet<_>>(),
        RapidHashSet::from_iter([leaf_one_cid, leaf_two_cid])
    );
}

/// A linear DAG deeper than the old fixed 20-iteration cap must still fully
/// reconcile. Each selective-fetch iteration reveals one deeper layer, so a
/// 25-deep chain needs 24 selective iterations; the previous `0..20` cap
/// abandoned it unmerged (no `DagReady`) even though every iteration was
/// still making progress.
#[tokio::test]
async fn poll_fetch_dag_completes_dag_deeper_than_legacy_iteration_cap() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));

    const DEPTH: usize = 25;

    // Build a linear chain nodes[0] (leaf) -> ... -> nodes[DEPTH-1] (root),
    // each node linking its single child by CID.
    let mut nodes: Vec<(Cid, Vec<u8>)> = Vec::with_capacity(DEPTH);
    let mut child: Option<Cid> = None;
    for i in 0..DEPTH {
        let data = match child {
            Some(c) => encode_ipld(ipld!({ "i": i as i64, "child": c })),
            None => encode_ipld(ipld!({ "i": i as i64 })),
        };
        let cid = make_cid(&data);
        child = Some(cid);
        nodes.push((cid, data));
    }
    let (root_cid, root_data) = nodes.last().unwrap().clone();

    // Root arrives via CAR; every ancestor is fetched one layer per iteration.
    let selective_blocks: RapidHashMap<Cid, Vec<u8>> = nodes[..DEPTH - 1]
        .iter()
        .map(|(cid, data)| (*cid, data.clone()))
        .collect();
    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        selective_blocks,
    );

    let (event_tx, mut event_rx) = mpsc::channel(1);
    let source_peer = PeerId::new("remote-peer".to_string());

    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            source_peer,
        ),
        DagFetchLimiter::new(2),
        diagnostics(),
    )
    .await;

    assert!(
        matches!(
            event_rx.recv().await,
            Some(SyncEvent::DagReady { root_cid: ready_cid, .. }) if ready_cid == root_cid
        ),
        "a DAG deeper than the legacy 20-iteration cap must fully reconcile and emit DagReady"
    );
    for (cid, _) in &nodes {
        assert!(matches!(blockstore.has(cid).await, Ok(true)));
    }
    // One selective iteration per ancestor: DEPTH - 1 batches.
    assert_eq!(transport.sync_batches().len(), DEPTH - 1);
}

fn single_child_dag() -> (Cid, Vec<u8>, Cid, Vec<u8>) {
    let child_data = encode_ipld(ipld!({ "value": 1 }));
    let child_cid = make_cid(&child_data);
    let root_data = encode_ipld(ipld!({ "child": child_cid }));
    let root_cid = make_cid(&root_data);
    (root_cid, root_data, child_cid, child_data)
}

/// A dead source peer must not fail the walk: the batch rotates to the
/// alternate provider and the fetch completes on the first attempt.
#[tokio::test(start_paused = true)]
async fn poll_fetch_dag_rotates_to_alternate_provider_on_no_progress() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (root_cid, root_data, child_cid, child_data) = single_child_dag();

    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(child_cid, child_data)]),
    );
    transport.mark_provider_dead("dead-peer");

    let (event_tx, mut event_rx) = mpsc::channel(1);
    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            PeerId::new("dead-peer".to_string()),
        )
        .with_alternate_providers(vec![PeerId::new("alt-peer".to_string())]),
        DagFetchLimiter::new(2),
        diagnostics(),
    )
    .await;

    assert!(matches!(
        event_rx.recv().await,
        Some(SyncEvent::DagReady { root_cid: ready_cid, .. }) if ready_cid == root_cid
    ));
    assert!(matches!(blockstore.has(&child_cid).await, Ok(true)));
    assert_eq!(transport.car_request_count(), 1);
    assert_eq!(
        transport.sync_providers(),
        vec!["dead-peer".to_string(), "alt-peer".to_string()]
    );
}

/// An attempt that stalls (no blocks served) must be retried after
/// backoff and succeed once the provider starts serving.
#[tokio::test(start_paused = true)]
async fn poll_fetch_dag_retries_incomplete_fetch_and_succeeds() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (root_cid, root_data, child_cid, child_data) = single_child_dag();

    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(child_cid, child_data)]),
    );
    transport.set_skip_serving_syncs(1);

    let (event_tx, mut event_rx) = mpsc::channel(1);
    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            PeerId::new("remote-peer".to_string()),
        ),
        DagFetchLimiter::new(2),
        diagnostics(),
    )
    .await;

    assert!(matches!(
        event_rx.recv().await,
        Some(SyncEvent::DagReady { root_cid: ready_cid, .. }) if ready_cid == root_cid
    ));
    assert!(matches!(blockstore.has(&child_cid).await, Ok(true)));
    // Only the root-absent first attempt needs recursive CAR discovery. Once
    // the root is local, retry uses the exact missing-CID frontier.
    assert_eq!(transport.car_request_count(), 1);
    assert_eq!(transport.sync_batches().len(), 2);
}

/// When every attempt stalls against every provider the fetcher stops
/// after MAX_FETCH_ATTEMPTS without emitting DagReady (terminal failure).
#[tokio::test(start_paused = true)]
async fn poll_fetch_dag_exhausted_retries_do_not_emit_dag_ready() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (root_cid, root_data, child_cid, child_data) = single_child_dag();

    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(child_cid, child_data)]),
    );
    transport.mark_provider_dead("dead-peer");

    let (event_tx, mut event_rx) = mpsc::channel(1);
    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            PeerId::new("dead-peer".to_string()),
        ),
        DagFetchLimiter::new(2),
        diagnostics(),
    )
    .await;

    assert!(
        event_rx.recv().await.is_none(),
        "terminal failure must not emit DagReady"
    );
    assert!(matches!(blockstore.has(&child_cid).await, Ok(false)));
    // Only the root-absent first attempt needs recursive CAR discovery. The
    // remaining attempts retry the exact missing-CID frontier.
    assert_eq!(transport.car_request_count(), 1);
    assert_eq!(transport.sync_batches().len(), MAX_FETCH_ATTEMPTS as usize);
}

/// A success-acked durable root can outlive its provider's live connection.
/// That interval belongs to the existing per-root retry clock: it must not
/// burn the inner fetch budget or be reported as terminal exhaustion. Once
/// the same qualified provider reconnects, a later clock dispatch completes.
#[tokio::test(start_paused = true)]
async fn disconnected_provider_defers_until_reconnect_without_exhaustion() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (root_cid, root_data, child_cid, child_data) = single_child_dag();
    blockstore.put(&root_cid, &root_data).await.unwrap();

    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(child_cid, child_data)]),
    );
    transport.set_connected_peers(Vec::new());
    let diagnostics = diagnostics();
    let context = DagFetchContext::new(
        "doc-id".to_string(),
        "collection-id".to_string(),
        "creator-id".to_string(),
        PeerId::new("remote-peer".to_string()),
    );

    let (event_tx, mut event_rx) = mpsc::channel(1);
    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        context.clone(),
        DagFetchLimiter::new(2),
        diagnostics.clone(),
    )
    .await;

    assert!(event_rx.recv().await.is_none());
    assert!(transport.sync_batches().is_empty());
    let deferred = diagnostics.snapshot();
    assert_eq!(deferred.pending_dag_fetch_deferred_unavailable, 1);
    assert_eq!(deferred.pending_dag_fetch_exhausted, 0);

    transport.set_connected_peers(vec![PeerId::new("remote-peer".to_string())]);
    let (event_tx, mut event_rx) = mpsc::channel(1);
    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        context,
        DagFetchLimiter::new(2),
        diagnostics.clone(),
    )
    .await;

    assert!(matches!(
        event_rx.recv().await,
        Some(SyncEvent::DagReady { root_cid: ready_cid, .. }) if ready_cid == root_cid
    ));
    assert!(matches!(blockstore.has(&child_cid).await, Ok(true)));
    let completed = diagnostics.snapshot();
    assert_eq!(completed.pending_dag_fetch_deferred_unavailable, 1);
    assert_eq!(completed.pending_dag_fetch_exhausted, 0);
}

/// Every issued block-sync query must be reaped via `cancel_sync` at the
/// end of its poll window: a stalled provider's transport-side work must
/// not outlive rotation, the limiter permit, or terminal failure.
#[tokio::test(start_paused = true)]
async fn poll_fetch_dag_cancels_every_issued_query() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (root_cid, root_data, child_cid, child_data) = single_child_dag();

    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(child_cid, child_data)]),
    );
    transport.mark_provider_dead("dead-peer");

    let (event_tx, mut event_rx) = mpsc::channel(1);
    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            PeerId::new("dead-peer".to_string()),
        ),
        DagFetchLimiter::new(2),
        diagnostics(),
    )
    .await;

    assert!(event_rx.recv().await.is_none());
    let issued: Vec<u64> = (1..=transport.sync_batches().len() as u64).collect();
    assert_eq!(
        transport.cancelled_queries(),
        issued,
        "every stalled query must be cancelled, in issue order"
    );
}

/// The per-attempt stall budget must cap stalled-batch work: once every
/// provider has burned a full window, remaining batches in the attempt
/// fail fast without issuing transport queries, so dead-provider attempt
/// time does not scale with the width of the missing frontier.
#[tokio::test(start_paused = true)]
async fn poll_fetch_dag_stall_budget_caps_stalled_batches_per_attempt() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));

    let width = SELECTIVE_FETCH_BATCH_SIZE + 1;
    let mut children = Vec::with_capacity(width);
    for i in 0..width {
        let data = encode_ipld(ipld!({ "value": i as i64 }));
        children.push(make_cid(&data));
    }
    let root_data = encode_ipld(Ipld::List(
        children.iter().map(|cid| Ipld::Link(*cid)).collect(),
    ));
    let root_cid = make_cid(&root_data);
    blockstore.put(&root_cid, &root_data).await.unwrap();

    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::new(),
    );
    transport.mark_provider_dead("dead-peer");

    let (event_tx, mut event_rx) = mpsc::channel(1);
    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            PeerId::new("dead-peer".to_string()),
        ),
        DagFetchLimiter::new(2),
        diagnostics(),
    )
    .await;

    assert!(event_rx.recv().await.is_none());
    // Two batches per attempt are missing, but the single provider's stall
    // budget is spent on the first, so the second issues no query: one
    // stalled (and cancelled) query per attempt, not two.
    let batches = transport.sync_batches();
    assert_eq!(batches.len(), MAX_FETCH_ATTEMPTS as usize);
    for batch in &batches {
        assert_eq!(batch.len(), SELECTIVE_FETCH_BATCH_SIZE);
    }
    assert_eq!(
        transport.cancelled_queries(),
        (1..=MAX_FETCH_ATTEMPTS as u64).collect::<Vec<_>>()
    );
}

/// A CAR request that never resolves (half-open peer: connected but
/// unresponsive) must not stall the attempt beyond the CAR budget — the
/// fetch falls back to the selective path and completes, instead of
/// waiting out the transport's full internal timeout chain.
#[tokio::test(start_paused = true)]
async fn poll_fetch_dag_bounds_hung_car_request() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (root_cid, root_data, child_cid, child_data) = single_child_dag();

    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data.clone(),
        RapidHashMap::new(),
        RapidHashMap::from_iter([(root_cid, root_data), (child_cid, child_data)]),
    );
    transport.set_hang_car_requests();

    let started = Instant::now();
    let (event_tx, mut event_rx) = mpsc::channel(1);
    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            PeerId::new("remote-peer".to_string()),
        ),
        DagFetchLimiter::new(2),
        diagnostics(),
    )
    .await;

    assert!(matches!(
        event_rx.recv().await,
        Some(SyncEvent::DagReady { root_cid: ready_cid, .. }) if ready_cid == root_cid
    ));
    assert!(matches!(blockstore.has(&child_cid).await, Ok(true)));
    assert_eq!(transport.car_request_count(), 1);
    assert!(
        started.elapsed() < Duration::from_secs(60),
        "hung CAR request must be cut at its budget, not awaited to transport timeouts; elapsed: {:?}",
        started.elapsed()
    );
}

/// A `connected_peers()` failure must degrade to an empty alternates list
/// (source-peer-only rotation), not abort the fetch: composed exactly as
/// the event-handler call sites do, the fetch still completes from the
/// source peer.
#[tokio::test(start_paused = true)]
async fn poll_fetch_dag_completes_from_source_when_peer_listing_fails() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (root_cid, root_data, child_cid, child_data) = single_child_dag();

    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(child_cid, child_data)]),
    );
    transport.set_fail_connected_peers();

    let alternate_providers = connected_alternate_providers(&transport, &root_cid).await;
    assert!(
        alternate_providers.is_empty(),
        "transport failure must degrade to no alternates"
    );

    let (event_tx, mut event_rx) = mpsc::channel(1);
    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            PeerId::new("remote-peer".to_string()),
        )
        .with_alternate_providers(alternate_providers),
        DagFetchLimiter::new(2),
        diagnostics(),
    )
    .await;

    assert!(matches!(
        event_rx.recv().await,
        Some(SyncEvent::DagReady { root_cid: ready_cid, .. }) if ready_cid == root_cid
    ));
    assert!(matches!(blockstore.has(&child_cid).await, Ok(true)));
    assert_eq!(transport.sync_providers(), vec!["remote-peer".to_string()]);
}

/// The limiter permit must be released during retry backoff: a second
/// waiter acquires the single permit while the first fetch is sleeping
/// between attempts, not after all of its retries complete.
#[tokio::test(start_paused = true)]
async fn poll_fetch_dag_releases_limiter_permit_during_backoff() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (root_cid, root_data, child_cid, child_data) = single_child_dag();

    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(child_cid, child_data)]),
    );
    transport.mark_provider_dead("dead-peer");

    let limiter = DagFetchLimiter::new(1);
    let (event_tx, mut event_rx) = mpsc::channel(1);
    let fetch = n0_future::task::spawn(poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            PeerId::new("dead-peer".to_string()),
        ),
        limiter.clone(),
        diagnostics(),
    ));

    // Let attempt 1 start (and therefore hold the only permit) before
    // competing for it.
    while transport.sync_batches().is_empty() {
        n0_future::time::sleep(Duration::from_millis(10)).await;
    }

    // Resolves as soon as attempt 1's permit drops (start of backoff);
    // if the permit spanned all attempts this would only resolve after
    // the fetch task finished.
    let permits = limiter
        .acquire(&PeerId::new("other-peer".to_string()))
        .await
        .expect("limiter must grant a permit while the fetch is backing off");
    assert!(
        !fetch.is_finished(),
        "fetch must still be mid-retry while another waiter holds the permit"
    );
    assert_eq!(transport.sync_batches().len(), 1);

    drop(permits);
    fetch.await.unwrap();

    assert!(event_rx.recv().await.is_none());
    assert_eq!(transport.sync_batches().len(), MAX_FETCH_ATTEMPTS as usize);
}

/// B0 rust-0 16:25:10.256-10.294: `Bitswap fetch complete success=true` is
/// processed 4 ms before the three `Stored Bitswap block` puts land, the poll
/// owner reports `Timeout fetching selective block batch` after a 13 ms window,
/// and the DAG then completes through the manager path 25 ms later. A Success
/// completion means every block has already been handed to this node; it must
/// not be read as a stall.
#[tokio::test(start_paused = true)]
async fn exact_selective_success_completion_waits_for_blocks_still_landing() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (root_cid, root_data, child_cid, child_data) = single_child_dag();
    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(child_cid, child_data)]),
    );
    let completion = crate::sync::manager::BlockSyncCompletionTracker::default();
    transport.set_early_success_completion(completion.clone());
    let context = DagFetchContext::new(
        "doc-id".to_string(),
        "collection-id".to_string(),
        "creator-id".to_string(),
        PeerId::new("remote-peer".to_string()),
    )
    .with_block_sync_completions(completion);
    let started = n0_future::time::Instant::now();

    let outcome = poll_fetch_blocks(
        &root_cid,
        &[child_cid],
        &transport,
        &blockstore,
        &PeerId::new("remote-peer".to_string()),
        &context,
    )
    .await;

    assert_eq!(
        outcome,
        ProviderWindowOutcome::Complete,
        "a successful completion whose blocks are still being stored is not a stall"
    );
    assert!(matches!(blockstore.has(&child_cid).await, Ok(true)));
    assert!(
        n0_future::time::Instant::now().duration_since(started) < Duration::from_secs(1),
        "the landing blocks are local; waiting for them must not cost a fetch window"
    );
}

/// The same ordering, one level up: the spurious stall costs the whole
/// dispatch a 2 s in-task backoff (`retry_backoff(2)`) while it holds one of
/// the four fetch slots, and the root is usually resolved by the manager path
/// before the retry even runs. At B0 saturation that sleep was ~70% of all
/// fetch-slot time.
#[tokio::test(start_paused = true)]
async fn success_completion_racing_block_storage_does_not_burn_a_retry_backoff() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (root_cid, root_data, child_cid, child_data) = single_child_dag();
    blockstore.put(&root_cid, &root_data).await.unwrap();
    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(child_cid, child_data)]),
    );
    let completion = crate::sync::manager::BlockSyncCompletionTracker::default();
    transport.set_early_success_completion(completion.clone());
    let (event_tx, mut event_rx) = mpsc::channel(1);
    let started = n0_future::time::Instant::now();

    poll_fetch_dag(
        transport.clone(),
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            PeerId::new("remote-peer".to_string()),
        )
        .with_block_sync_completions(completion),
        DagFetchLimiter::new(2),
        diagnostics(),
    )
    .await;

    assert!(matches!(
        event_rx.recv().await,
        Some(SyncEvent::DagReady { root_cid: ready_cid, .. }) if ready_cid == root_cid
    ));
    let elapsed = n0_future::time::Instant::now().duration_since(started);
    assert!(
        elapsed < Duration::from_secs(1),
        "fetch slot held {elapsed:?}: the first attempt slept through retry_backoff(2) \
         for blocks that had already been delivered"
    );
}

/// The same race, but the completion arrives in the watchdog's final moments:
/// its blocks then land after `BLOCK_SYNC_COMPLETION_WATCHDOG` would have
/// expired. The grace has to outlive the watchdog, or a Success arriving late
/// in the window gets no grace at all and burns the retry backoff anyway.
#[tokio::test(start_paused = true)]
async fn late_success_completion_keeps_the_full_landing_grace() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (root_cid, root_data, child_cid, child_data) = single_child_dag();
    // The transport serves nothing: this test drives the completion and the
    // block landing itself, on the production ordering.
    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::new(),
    );
    let completion = crate::sync::manager::BlockSyncCompletionTracker::default();
    let context = DagFetchContext::new(
        "doc-id".to_string(),
        "collection-id".to_string(),
        "creator-id".to_string(),
        PeerId::new("remote-peer".to_string()),
    )
    .with_block_sync_completions(completion.clone());

    let landing_store = blockstore.clone();
    n0_future::task::spawn(async move {
        n0_future::time::sleep(BLOCK_SYNC_COMPLETION_WATCHDOG - Duration::from_millis(150)).await;
        // First (and only) sync_blocks call of this poll window.
        completion.complete(QueryId(1), true);
        n0_future::time::sleep(SUCCESS_LANDING_GRACE / 2).await;
        landing_store.put(&child_cid, &child_data).await.unwrap();
    });

    let outcome = poll_fetch_blocks(
        &root_cid,
        &[child_cid],
        &transport,
        &blockstore,
        &PeerId::new("remote-peer".to_string()),
        &context,
    )
    .await;

    assert_eq!(
        outcome,
        ProviderWindowOutcome::Complete,
        "a Success in the watchdog's last moments must still wait out the landing grace"
    );
    assert!(matches!(blockstore.has(&child_cid).await, Ok(true)));
}
