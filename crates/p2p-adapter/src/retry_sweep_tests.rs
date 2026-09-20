//! Sweep-shape contracts for `run_retry_pass`: one pass over a peer's due
//! markers must keep going past a per-document failure, treat a receiver's
//! backpressure as a wait rather than a verdict, and pay at most one ladder
//! rung per pass, returning to the first rung once documents land — while
//! staying bounded, so one unresponsive peer cannot hold the serial sweep.

use kovan::Atom;
use rapidhash::RapidHashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cid::Cid;
use p2p::message::{
    BranchableSyncReply, BranchableSyncRequest, DocSyncReply, DocSyncRequest,
    PushSEArtifactsRequest,
};
use p2p::transport::{MessageId, PeerAddr, PeerId};
use p2p::{DefraTopic, PushLogBroadcast, PushLogReply, PushLogRequest, QueryId, ReplicatorInfo};
use storage::backends::RegolithStore;
use storage::stores::retry_info::RETRY_INTERVALS_SECS;
use storage::stores::{Peerstore, RetryInfo};

use crate::retry::run_retry_pass;
use crate::transport_doc_pusher::TransportDocPusher;
use crate::{P2PError, P2PErrorExt as _, P2PResult};

const PEER: &str = "peer-a";
const OTHER_PEER: &str = "peer-b";

/// A failure the replay path attributed to the document, not the transport:
/// `push_docs` picks this prefix when the typed error is not connection-like.
const DOCUMENT_FAILURE: &str =
    "replay push failed after 0 successful block(s): failed to write request: 9ddf7e88/1451: connection is closed";
const RATE_LIMITED: &str =
    "peer rejected replay after 0 successful block(s): rate limited: too many requests, retry later";

#[derive(Clone)]
enum Outcome {
    Delivered,
    Failed(&'static str),
    RateLimited(u64),
    WaitForAck(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>),
    Hangs,
}

/// Replays documents by script and, like the production pusher, clears the
/// marker of every delivered document.
struct ScriptedPusher {
    peerstore: Peerstore<RegolithStore>,
    script: RapidHashMap<String, Outcome>,
    attempted: Atom<Vec<String>>,
}

impl ScriptedPusher {
    fn attempted(&self) -> Vec<String> {
        self.attempted.load_clone()
    }
}

#[async_trait]
impl TransportDocPusher for ScriptedPusher {
    async fn retry_doc(
        &self,
        peer_id: &PeerId,
        doc_id: &str,
        _collection_id: &str,
    ) -> P2PResult<()> {
        self.attempted.rcu(|attempted| {
            let mut next = attempted.clone();
            next.push(doc_id.to_string());
            next
        });
        match self
            .script
            .get(doc_id)
            .cloned()
            .unwrap_or(Outcome::Delivered)
        {
            Outcome::Delivered => {
                self.peerstore
                    .complete_retry_scope(peer_id.as_str(), doc_id, "", false)
                    .await
                    .unwrap();
                Ok(())
            }
            Outcome::Failed(message) => Err(P2PError::internal(message)),
            Outcome::RateLimited(millis) => {
                let mut reply = PushLogReply::error("request", p2p::error::RATE_LIMITED_MESSAGE);
                reply.retry_after_ms = Some(millis);
                db::merge::push_docs_replay::persist_retry_after(&self.peerstore, peer_id, &reply)
                    .await
                    .unwrap();
                Err(P2PError::internal(RATE_LIMITED))
            }
            Outcome::WaitForAck(started, finish) => {
                started.notify_one();
                finish.notified().await;
                self.peerstore
                    .complete_retry_scope(peer_id.as_str(), doc_id, "", false)
                    .await
                    .unwrap();
                Ok(())
            }
            Outcome::Hangs => std::future::pending().await,
        }
    }

    async fn retry_collection_commit(
        &self,
        _peer_id: &PeerId,
        _collection_id: &str,
    ) -> P2PResult<()> {
        unreachable!("document markers only")
    }

    async fn push_existing_docs(
        &self,
        _peer_id: &PeerId,
        _collections: &[String],
        _filters: &p2p::ReplicationFilters,
        _se_key: Option<&[u8]>,
        _se_identity_pubkey: Option<&[u8]>,
    ) -> P2PResult<()> {
        unreachable!()
    }
    async fn load_document_head_blocks(
        &self,
        _doc_id: &str,
    ) -> P2PResult<Vec<(Cid, bytes::Bytes)>> {
        unreachable!()
    }
    async fn load_doc_creator_did(&self, _c: &str, _d: &str) -> P2PResult<Option<String>> {
        unreachable!()
    }
    fn get_collection_id(&self, _name: &str) -> Option<String> {
        unreachable!()
    }
    fn get_collection_name(&self, _collection_id: &str) -> P2PResult<Option<String>> {
        unreachable!()
    }
    fn list_collections(&self) -> P2PResult<Vec<String>> {
        unreachable!()
    }
    async fn persist_replicator(&self, _p: &str, _c: &[String]) -> P2PResult<()> {
        unreachable!()
    }
    async fn delete_persisted_replicator(&self, _p: &str) -> P2PResult<()> {
        unreachable!()
    }
    async fn persist_p2p_documents(&self, _d: &[String]) -> P2PResult<()> {
        unreachable!()
    }
    async fn load_p2p_documents(&self) -> P2PResult<Vec<String>> {
        unreachable!()
    }
    async fn persist_p2p_collections(&self, _c: &[String]) -> P2PResult<()> {
        unreachable!()
    }
    fn validate_collection_exists(&self, _name: &str) -> P2PResult<()> {
        unreachable!()
    }
    fn validate_branchable_collection(&self, _id: &str) -> P2PResult<()> {
        unreachable!()
    }
}

/// A transport whose only job is to report the sweep's peers as connected.
#[derive(Clone)]
struct ConnectedTransport {
    local: PeerId,
    connected: Vec<PeerId>,
}

#[async_trait]
impl p2p::P2PTransport for ConnectedTransport {
    type ResponseToken = ();

    fn local_peer_id(&self) -> &PeerId {
        &self.local
    }
    fn local_public_key_proto(&self) -> &[u8] {
        &[]
    }
    fn sign(&self, _data: &[u8]) -> p2p::Result<Vec<u8>> {
        Ok(Vec::new())
    }
    async fn dial(&self, _peer_id: &PeerId, _addrs: Vec<PeerAddr>) -> p2p::Result<()> {
        unreachable!("every sweep peer is connected")
    }
    async fn disconnect(&self, _peer_id: &PeerId) -> p2p::Result<()> {
        unreachable!()
    }
    async fn listen(&self, _addr: PeerAddr) -> p2p::Result<()> {
        unreachable!()
    }
    async fn connected_peers(&self) -> p2p::Result<Vec<PeerId>> {
        Ok(self.connected.clone())
    }
    async fn listen_addresses(&self) -> p2p::Result<Vec<PeerAddr>> {
        unreachable!()
    }
    async fn poll_until_connected(&self, _p: &PeerId, _t: Duration) -> p2p::Result<()> {
        unreachable!()
    }
    async fn peer_addresses(&self) -> p2p::Result<Vec<String>> {
        unreachable!()
    }
    async fn subscribe(&self, _topic: DefraTopic) -> p2p::Result<bool> {
        unreachable!()
    }
    async fn unsubscribe(&self, _topic: DefraTopic) -> p2p::Result<bool> {
        unreachable!()
    }
    async fn publish(&self, _t: DefraTopic, _m: PushLogBroadcast) -> p2p::Result<MessageId> {
        unreachable!()
    }
    async fn topic_peers(&self, _topic: DefraTopic) -> p2p::Result<Vec<PeerId>> {
        unreachable!()
    }
    async fn send_pushlog_response(&self, _t: (), _r: PushLogReply) -> p2p::Result<()> {
        unreachable!()
    }
    async fn send_two_stream_request(
        &self,
        _p: &PeerId,
        _r: PushLogRequest,
    ) -> p2p::Result<PushLogReply> {
        unreachable!()
    }
    async fn send_two_stream_response(&self, _p: &PeerId, _r: PushLogReply) -> p2p::Result<()> {
        unreachable!()
    }
    async fn send_doc_sync_request(&self, _p: &PeerId, _r: DocSyncRequest) -> p2p::Result<()> {
        unreachable!()
    }
    async fn send_doc_sync_response(&self, _p: &PeerId, _r: DocSyncReply) -> p2p::Result<()> {
        unreachable!()
    }
    async fn send_branchable_sync_request(
        &self,
        _p: &PeerId,
        _r: BranchableSyncRequest,
    ) -> p2p::Result<()> {
        unreachable!()
    }
    async fn send_branchable_sync_response(
        &self,
        _p: &PeerId,
        _r: BranchableSyncReply,
    ) -> p2p::Result<()> {
        unreachable!()
    }
    async fn send_car_request(&self, _p: &PeerId, _c: Cid) -> p2p::Result<()> {
        unreachable!()
    }
    async fn send_car_response(&self, _p: &PeerId, _d: Vec<u8>) -> p2p::Result<()> {
        unreachable!()
    }
    async fn send_car_response_token(&self, _t: (), _d: Vec<u8>) -> p2p::Result<()> {
        unreachable!()
    }
    async fn send_doc_sync_response_token(&self, _t: (), _r: DocSyncReply) -> p2p::Result<()> {
        unreachable!()
    }
    async fn send_branchable_sync_response_token(
        &self,
        _t: (),
        _r: BranchableSyncReply,
    ) -> p2p::Result<()> {
        unreachable!()
    }
    async fn send_se_artifacts(&self, _p: &PeerId, _r: PushSEArtifactsRequest) -> p2p::Result<()> {
        unreachable!()
    }
    async fn sync_blocks(&self, _r: Cid, _p: Vec<PeerId>, _m: Vec<Cid>) -> p2p::Result<QueryId> {
        unreachable!()
    }
    async fn cancel_sync(&self, _q: QueryId) -> p2p::Result<bool> {
        unreachable!()
    }
    async fn create_replicator(&self, _p: &PeerId, _c: Vec<String>) -> p2p::Result<()> {
        unreachable!()
    }
    async fn delete_replicator(&self, _p: &PeerId) -> p2p::Result<()> {
        unreachable!()
    }
    async fn list_replicators(&self) -> p2p::Result<Vec<ReplicatorInfo>> {
        unreachable!()
    }
    async fn get_replicator(&self, _p: &PeerId) -> p2p::Result<Option<ReplicatorInfo>> {
        unreachable!()
    }
    async fn remove_replicator_collections(
        &self,
        _p: &PeerId,
        _c: Vec<String>,
    ) -> p2p::Result<bool> {
        unreachable!()
    }
    async fn shutdown(&self) -> p2p::Result<()> {
        unreachable!()
    }
}

struct Sweep {
    peerstore: Peerstore<RegolithStore>,
    pusher: Arc<ScriptedPusher>,
    transport: ConnectedTransport,
}

impl Sweep {
    /// `peer-a` holds one due marker per document in `docs`, on a fresh ladder.
    async fn with_markers(docs: &[&str], script: &[(&str, Outcome)]) -> Self {
        Self::with_peers(
            &[(PEER, docs.iter().map(|doc| doc.to_string()).collect())],
            script
                .iter()
                .map(|(doc, outcome)| (doc.to_string(), outcome.clone()))
                .collect(),
        )
        .await
    }

    /// Every peer holds one due marker per document, all on a fresh ladder.
    async fn with_peers(
        peers: &[(&str, Vec<String>)],
        script: RapidHashMap<String, Outcome>,
    ) -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let peerstore = Peerstore::new(Arc::clone(&store));
        let initial = RetryInfo::new_initial().to_bytes().unwrap();
        for (peer, docs) in peers {
            let replicator = ReplicatorInfo::from_raw(
                peer.to_string(),
                vec!["collection-a".to_string()],
                Vec::new(),
            );
            peerstore
                .create_replicator(peer, &replicator.to_bytes().unwrap())
                .await
                .unwrap();
            for doc in docs {
                peerstore
                    .record_push_failure(peer, doc, "collection-a", &initial)
                    .await
                    .unwrap();
            }
            peerstore.activate_retry_peer(peer).await.unwrap();
        }
        let pusher = Arc::new(ScriptedPusher {
            peerstore: Peerstore::new(Arc::clone(&store)),
            script,
            attempted: Atom::new(Vec::new()),
        });
        Self {
            peerstore,
            pusher,
            transport: ConnectedTransport {
                local: PeerId::new("local".to_string()),
                connected: peers
                    .iter()
                    .map(|(peer, _)| PeerId::new(peer.to_string()))
                    .collect(),
            },
        }
    }

    /// Escalate the peer clock `rungs` times, then make it due again.
    async fn escalate(&self, rungs: usize) {
        for _ in 0..rungs {
            self.peerstore
                .reschedule_retry_peer(PEER, None, 1)
                .await
                .unwrap();
        }
        self.peerstore.activate_retry_peer(PEER).await.unwrap();
    }

    async fn run(&self) {
        let pusher: Arc<dyn TransportDocPusher> =
            Arc::clone(&self.pusher) as Arc<dyn TransportDocPusher>;
        run_retry_pass(&self.peerstore, &self.transport, &pusher, None, false).await;
    }

    async fn retry_info(&self) -> RetryInfo {
        RetryInfo::from_bytes(&self.peerstore.get_retry_info(PEER).await.unwrap().unwrap()).unwrap()
    }

    async fn seconds_until_due(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.retry_info().await.next_retry_unix.saturating_sub(now)
    }

    async fn remaining_markers(&self) -> Vec<String> {
        self.markers_for(PEER).await
    }

    async fn markers_for(&self, peer: &str) -> Vec<String> {
        let mut docs: Vec<String> = self
            .peerstore
            .get_retry_documents(peer)
            .await
            .unwrap()
            .into_iter()
            .map(|marker| marker.doc_id)
            .collect();
        docs.sort();
        docs
    }
}

#[tokio::test]
async fn document_failures_do_not_abandon_the_remaining_due_markers() {
    let sweep = Sweep::with_markers(
        &["a", "b", "c", "d", "e", "f"],
        &[
            ("a", Outcome::Failed(DOCUMENT_FAILURE)),
            ("b", Outcome::Failed(DOCUMENT_FAILURE)),
            ("c", Outcome::Failed(DOCUMENT_FAILURE)),
        ],
    )
    .await;

    sweep.run().await;

    let mut attempted = sweep.pusher.attempted();
    attempted.sort();
    assert_eq!(
        attempted,
        ["a", "b", "c", "d", "e", "f"],
        "three per-document failures abandoned the rest of the due set"
    );
    assert_eq!(sweep.remaining_markers().await, ["a", "b", "c"]);
}

/// A replay timeout is evidence about the peer, not the document: the rest of
/// the set would buy one `REPLAY_TIMEOUT` each for the same answer. The markers
/// are kept and the durable cursor moves past the silent one, so the next pass
/// resumes on the tail rather than re-hanging on the head.
#[tokio::test(start_paused = true)]
async fn a_replay_timeout_stops_the_peer_pass_and_resumes_past_the_hung_marker() {
    let sweep = Sweep::with_markers(&["a", "b", "c"], &[("a", Outcome::Hangs)]).await;

    sweep.run().await;

    assert_eq!(
        sweep.pusher.attempted(),
        ["a"],
        "a silent peer was charged a replay timeout for every remaining marker"
    );
    assert_eq!(sweep.remaining_markers().await, ["a", "b", "c"]);

    sweep.peerstore.activate_retry_peer(PEER).await.unwrap();
    sweep.run().await;

    assert_eq!(
        sweep.pusher.attempted(),
        ["a", "b", "c", "a"],
        "the next pass restarted on the hung marker instead of resuming past it"
    );
}

/// One peer's pass is capped, and the durable dispatch cursor makes the next
/// pass pick up the tail instead of re-walking the head of the set.
#[tokio::test]
async fn a_peer_pass_is_bounded_and_the_next_pass_resumes_where_it_stopped() {
    let bound = crate::retry::MAX_MARKERS_PER_PEER_PASS;
    let docs: Vec<String> = (0..bound * 2).map(|i| format!("doc-{i:03}")).collect();
    let script = docs
        .iter()
        .map(|doc| (doc.clone(), Outcome::Failed(DOCUMENT_FAILURE)))
        .collect();
    let sweep = Sweep::with_peers(&[(PEER, docs.clone())], script).await;

    sweep.run().await;

    assert_eq!(
        sweep.pusher.attempted(),
        docs[..bound],
        "one peer pass was not bounded by the marker cap"
    );

    sweep.peerstore.activate_retry_peer(PEER).await.unwrap();
    sweep.run().await;

    assert_eq!(
        sweep.pusher.attempted()[bound],
        docs[bound],
        "the second pass restarted at the head of the set and starved its tail"
    );
}

/// The regression the bounded pass exists for: peers are visited serially, so
/// before the bound a black-holed peer holding the measured 3,488 markers could
/// hold one pass for `3_488 * 15 s` — about 14.5 hours — and no later peer was
/// visited at all.
#[tokio::test(start_paused = true)]
async fn one_hung_peer_does_not_delay_a_healthy_peer() {
    let hung: Vec<String> = (0..40).map(|i| format!("a-{i:02}")).collect();
    let healthy: Vec<String> = (0..3).map(|i| format!("b-{i}")).collect();
    let script = hung
        .iter()
        .map(|doc| (doc.clone(), Outcome::Hangs))
        .collect();
    let sweep = Sweep::with_peers(
        &[(PEER, hung.clone()), (OTHER_PEER, healthy.clone())],
        script,
    )
    .await;

    let started = tokio::time::Instant::now();
    sweep.run().await;
    let elapsed = started.elapsed();

    let stranded = sweep.markers_for(OTHER_PEER).await;
    assert!(
        stranded.is_empty(),
        "the healthy peer went unserved behind a hung peer: {stranded:?}"
    );
    assert_eq!(
        sweep.markers_for(PEER).await.len(),
        hung.len(),
        "the hung peer lost markers it never delivered"
    );
    // Stopping on the timeout costs one REPLAY_TIMEOUT, well inside the pass
    // budget that would otherwise cap a peer that keeps answering slowly.
    assert!(
        elapsed < crate::retry::MAX_PEER_PASS,
        "one hung peer held the serial sweep for {elapsed:?}"
    );
}

#[tokio::test]
async fn a_rate_limited_reply_is_backpressure_not_a_ladder_failure() {
    let sweep = Sweep::with_markers(&["a"], &[("a", Outcome::Failed(RATE_LIMITED))]).await;
    let rung_before = sweep.retry_info().await.num_retries;

    sweep.run().await;

    let info = sweep.retry_info().await;
    assert_eq!(
        info.num_retries, rung_before,
        "a rate-limited reply advanced the failure ladder"
    );
    assert!(
        sweep.seconds_until_due().await <= p2p::sync::PERSISTED_RETRY_SWEEP_INTERVAL.as_secs() + 1,
        "a rate-limited reply parked the peer past the paced sweep: due in {} s",
        sweep.seconds_until_due().await
    );
}

#[tokio::test]
async fn retry_after_survives_reconnect_and_forced_sweep() {
    let sweep = Sweep::with_markers(&["a", "b"], &[("a", Outcome::RateLimited(45_000))]).await;
    let rung = sweep.retry_info().await.num_retries;
    sweep.run().await;
    assert_eq!(sweep.pusher.attempted(), vec!["a"]);
    let info = sweep.retry_info().await;
    assert_eq!(info.num_retries, rung);
    assert!(sweep.seconds_until_due().await >= 45);
    sweep.peerstore.activate_retry_peer(PEER).await.unwrap();
    let pusher: Arc<dyn TransportDocPusher> = sweep.pusher.clone();
    run_retry_pass(&sweep.peerstore, &sweep.transport, &pusher, None, true).await;
    assert_eq!(sweep.pusher.attempted(), vec!["a"]);
    assert_eq!(
        sweep.retry_info().await.not_before_unix,
        info.not_before_unix
    );
    assert_eq!(sweep.markers_for(PEER).await.len(), 2);
}

#[tokio::test]
async fn retry_after_arriving_during_ack_blocks_next_marker() {
    let started = Arc::new(tokio::sync::Notify::new());
    let finish = Arc::new(tokio::sync::Notify::new());
    let sweep = Arc::new(
        Sweep::with_markers(
            &["a", "b"],
            &[("a", Outcome::WaitForAck(started.clone(), finish.clone()))],
        )
        .await,
    );
    let runner = sweep.clone();
    let task = tokio::spawn(async move { runner.run().await });
    started.notified().await;
    sweep
        .peerstore
        .reschedule_retry_peer(PEER, Some(Duration::from_secs(45)), 0)
        .await
        .unwrap();
    finish.notify_one();
    task.await.unwrap();
    assert_eq!(sweep.pusher.attempted(), vec!["a"]);
    assert_eq!(sweep.markers_for(PEER).await.len(), 1);
    assert!(!sweep.retry_info().await.is_backpressure_elapsed());
}

#[tokio::test]
async fn one_pass_advances_the_ladder_by_at_most_one_rung() {
    let sweep = Sweep::with_markers(
        &["a", "b", "c"],
        &[
            ("a", Outcome::Failed(DOCUMENT_FAILURE)),
            ("b", Outcome::Failed(DOCUMENT_FAILURE)),
            ("c", Outcome::Failed(DOCUMENT_FAILURE)),
        ],
    )
    .await;
    let rung_before = sweep.retry_info().await.num_retries;

    sweep.run().await;

    assert_eq!(
        sweep.retry_info().await.num_retries,
        rung_before + 1,
        "one pass climbed more than one rung"
    );
}

#[tokio::test]
async fn a_pass_that_delivered_documents_returns_the_peer_to_the_first_rung() {
    let sweep = Sweep::with_markers(
        &["a", "b", "c"],
        &[("a", Outcome::Failed(DOCUMENT_FAILURE))],
    )
    .await;
    sweep.escalate(6).await;
    assert!(sweep.retry_info().await.num_retries >= RETRY_INTERVALS_SECS.len() as u32);

    sweep.run().await;

    assert_eq!(sweep.remaining_markers().await, ["a"]);
    let due_in = sweep.seconds_until_due().await;
    assert!(
        due_in <= RETRY_INTERVALS_SECS[0],
        "the receiver took two documents but the peer stays on the escalated rung: due in {due_in} s"
    );
}
