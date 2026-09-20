//! One durable sender retry state machine shared by every runtime and transport.

use std::sync::Arc;
use std::time::Duration;

use p2p::transport::{P2PTransport, PeerAddr, PeerId};
use rapidhash::{HashMapExt, RapidHashMap, RapidHashSet};

use crate::TransportDocPusher;

/// How long one marker's replay may run before the peer is presumed silent.
const REPLAY_TIMEOUT: Duration = Duration::from_secs(15);

/// Markers one peer may consume per pass.
///
/// Peers are visited serially, so an unbounded pass is a starvation path: a
/// black-holed peer holding the measured 3,488 markers costs
/// `3_488 * REPLAY_TIMEOUT` — about 14.5 hours — during which no later peer is
/// visited at all. The remainder is not dropped; the durable dispatch cursor
/// resumes there on the next pass.
const MAX_MARKERS_PER_PEER_PASS: usize = 64;

/// Wall-clock a peer may hold the sweep, checked between markers. One in-flight
/// replay can overrun it by at most `REPLAY_TIMEOUT`.
const MAX_PEER_PASS: Duration = Duration::from_secs(30);

/// A saturated or rate-limiting receiver is backpressure, not a verdict on
/// the document: wait one paced sweep, never a ladder rung.
fn capacity_retry_delay(error: &str) -> Option<std::time::Duration> {
    let is_backpressure = carries_reply_sentinel(error, p2p::error::AT_CAPACITY_MESSAGE)
        || carries_reply_sentinel(error, p2p::error::RATE_LIMITED_MESSAGE);
    is_backpressure.then_some(p2p::sync::PERSISTED_RETRY_SWEEP_INTERVAL)
}

/// The replay path reports a receiver's nack as `"<context>: <sentinel>"`, so an
/// exact sentinel is either the whole message or its `": "`-delimited tail.
/// Matched exactly, never as a loose substring: a document whose own content
/// mentions one of these phrases must not be read as backpressure.
fn carries_reply_sentinel(error: &str, sentinel: &str) -> bool {
    error == sentinel
        || error
            .strip_suffix(sentinel)
            .is_some_and(|prefix| prefix.ends_with(": "))
}

/// True when the replay never reached the receiver.
///
/// That is evidence about the peer, not the document, so the rest of the peer's
/// markers would only buy one `REPLAY_TIMEOUT` each for the same answer.
fn is_peer_level_failure(error: &str) -> bool {
    error.starts_with(p2p::error::TRANSPORT_UNAVAILABLE_PREFIX)
}

/// Record head announcements and acknowledgements behind the peer-scoped
/// retry writer. A success acknowledgement clears only the marker still
/// covered by the acknowledged head fence.
pub fn spawn_failure_recorder<S: storage::corekv::Store + 'static>(
    peerstore: storage::stores::Peerstore<S>,
    mut failures: tokio::sync::mpsc::Receiver<p2p::sync::PushFailure>,
) -> n0_future::task::JoinHandle<()> {
    n0_future::task::spawn(async move {
        let mut ack_fence = p2p::sync::HeadAckFence::default();
        while let Some(mut failure) = failures.recv().await {
            let durable_tx = failure.durable_tx.take();
            if failure.acknowledged && !ack_fence.ack_is_current(&failure) {
                tracing::debug!(
                    peer_id = %failure.peer_id,
                    doc_id = %failure.doc_id,
                    collection_id = %failure.collection_id,
                    "Ignoring stale head acknowledgement"
                );
                let _ = durable_tx.map(|tx| tx.send(false));
                continue;
            }

            let _retry_guard = match peerstore
                .acquire_replicator_retry_guard(&failure.peer_id)
                .await
            {
                Ok(Some(guard)) => guard,
                Ok(None) => {
                    let _ = durable_tx.map(|tx| tx.send(false));
                    continue;
                }
                Err(error) => {
                    tracing::warn!(%error, "failed to coordinate push failure recording");
                    let _ = durable_tx.map(|tx| tx.send(false));
                    continue;
                }
            };

            if failure.admission_only {
                let allowed = match peerstore.get_retry_info(&failure.peer_id).await {
                    Ok(Some(bytes)) => storage::stores::RetryInfo::from_bytes(&bytes)
                        .is_ok_and(|info| info.is_backpressure_elapsed()),
                    Ok(None) => true,
                    Err(error) => {
                        tracing::warn!(%error, "failed to read live push admission deadline");
                        false
                    }
                };
                let _ = durable_tx.map(|tx| tx.send(allowed));
                continue;
            }

            let result = if failure.acknowledged {
                peerstore
                    .complete_retry_scope(
                        &failure.peer_id,
                        &failure.doc_id,
                        &failure.collection_id,
                        failure.doc_id.is_empty(),
                    )
                    .await
            } else if failure.create_retry {
                let mut info = storage::stores::RetryInfo::new_initial();
                if let Some(delay) = failure.retry_after {
                    info.defer_for(delay);
                    tracing::debug!(target: "p2p::retry_after", peer_id = %failure.peer_id,
                        retry_after_ms = delay.as_millis() as u64,
                        retry_not_before_unix = info.not_before_unix,
                        "Persisting negotiated PushLog retry-after");
                }
                let info_bytes = match info.to_bytes() {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        tracing::warn!(%error, "failed to serialize retry info");
                        let _ = durable_tx.map(|tx| tx.send(false));
                        continue;
                    }
                };
                peerstore
                    .record_push_failure(
                        &failure.peer_id,
                        &failure.doc_id,
                        &failure.collection_id,
                        &info_bytes,
                    )
                    .await
            } else {
                peerstore
                    .observe_push_head(&failure.peer_id, &failure.doc_id, &failure.collection_id)
                    .await
            };
            if let Err(error) = result {
                tracing::warn!(%error, "failed to record push failure");
                let _ = durable_tx.map(|tx| tx.send(false));
                continue;
            }

            if !failure.create_retry && !failure.acknowledged {
                ack_fence.observe_durable(&failure);
            }
            let _ = durable_tx.map(|tx| tx.send(true));
            if failure.acknowledged {
                ack_fence.clear_current_ack(&failure);
                let _ = peerstore.clear_retry_peer(&failure.peer_id).await;
            } else if failure.create_retry {
                if let Err(error) = crate::set_persisted_replicator_status(
                    &peerstore,
                    &failure.peer_id,
                    p2p::ReplicatorStatus::Inactive,
                )
                .await
                {
                    tracing::warn!(%error, "failed to mark replicator inactive");
                }
            }
        }
    })
}

/// Activate a dormant durable peer schedule after either transport reconnects.
pub async fn activate_retry_peer<S: storage::corekv::Store>(store: Arc<S>, peer_id: &PeerId) {
    let peerstore = storage::stores::Peerstore::new(store);
    match peerstore.activate_retry_peer(peer_id.as_str()).await {
        Ok(true) => {
            tracing::debug!(%peer_id, "Activated durable push markers after peer reconnect")
        }
        Ok(false) => {}
        Err(error) => {
            tracing::warn!(%peer_id, %error, "failed to activate durable push markers after peer reconnect")
        }
    }
}

async fn redial_replicator<S, T>(
    peerstore: &storage::stores::Peerstore<S>,
    transport: &T,
    peer_id: &PeerId,
) where
    S: storage::corekv::Store,
    T: P2PTransport,
{
    let Ok(Some(bytes)) = peerstore.get_replicator(peer_id.as_str()).await else {
        return;
    };
    let Ok(info) = p2p::ReplicatorInfo::from_bytes(&bytes) else {
        return;
    };
    let addrs: Vec<PeerAddr> = info
        .addresses
        .iter()
        .filter_map(|address| transport.parse_dial_addr(address).ok())
        .filter(|(addressed_peer, _)| addressed_peer == peer_id)
        .flat_map(|(_, addrs)| addrs)
        .collect();
    match n0_future::time::timeout(RECONNECT_DIAL_TIMEOUT, transport.dial(peer_id, addrs)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::debug!(%peer_id, %error, "replicator retry redial failed"),
        Err(_) => tracing::debug!(%peer_id, "replicator retry redial timed out"),
    }
}

/// Ceiling on a single reconnect dial. Without it one unresponsive address
/// stalls every peer queued behind it on the same pass.
const RECONNECT_DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// First delay after a failed probe, doubling up to `RECONNECT_BACKOFF_MAX`.
/// Deliberately the sweep interval, because that is the tick this probe runs
/// on: a shorter floor would round up to the next pass and change nothing, so
/// shortening the sweep is meant to lower this floor with it.
const RECONNECT_BACKOFF_MIN: std::time::Duration = p2p::sync::PERSISTED_RETRY_SWEEP_INTERVAL;
/// Ceiling on the per-peer probe backoff.
const RECONNECT_BACKOFF_MAX: std::time::Duration = std::time::Duration::from_secs(60);
/// Dials one pass may start. A fleet-wide outage costs a bounded amount of
/// work per sweep rather than one dial per replicator.
const RECONNECT_DIALS_PER_PASS: usize = 8;

/// When a disconnected peer may next be probed, and how far its backoff has
/// walked. Held by the reconnect loop only; nothing here is durable.
struct ReconnectProbe {
    next_attempt: n0_future::time::Instant,
    backoff: std::time::Duration,
}

impl ReconnectProbe {
    fn new(now: n0_future::time::Instant) -> Self {
        Self {
            next_attempt: now,
            backoff: RECONNECT_BACKOFF_MIN,
        }
    }

    /// Charge the backoff *before* dialling, so a dial that hangs to its
    /// timeout still defers the next probe instead of retrying immediately.
    fn charge(&mut self, now: n0_future::time::Instant) {
        self.next_attempt = now + self.backoff;
        self.backoff = (self.backoff * 2).min(RECONNECT_BACKOFF_MAX);
    }
}

/// Probe replicator peers that are not connected, on a schedule of its own.
///
/// This exists because a node returning from a partition otherwise waits out
/// whatever rung the last push timeout charged on the durable replay ladder
/// before anything redials it, which measured 21.5s p50 on a 60s outage. The
/// probe never writes the retry schedule: a successful dial raises
/// `PeerConnected`, and that event remains the signal that activates the
/// durable markers (`activate_retry_peer`).
async fn run_reconnect_pass<S, T>(
    peerstore: &storage::stores::Peerstore<S>,
    transport: &T,
    probes: &mut RapidHashMap<String, ReconnectProbe>,
) where
    S: storage::corekv::Store,
    T: P2PTransport,
{
    let Ok(peers) = peerstore.get_replicator_retry_peers().await else {
        return;
    };
    let connected = match transport.connected_peers().await {
        Ok(connected) => connected,
        // No observation means no evidence, and a redial storm across the whole
        // replicator set is the worst thing to do on a transport hiccup.
        Err(error) => {
            tracing::debug!(%error, "peer observation failed; skipping reconnect probes");
            return;
        }
    };
    let scheduled: RapidHashSet<&str> = peers.iter().map(|(peer_id, _)| peer_id.as_str()).collect();
    probes.retain(|peer_id, _| scheduled.contains(peer_id.as_str()));

    let now = n0_future::time::Instant::now();
    let mut dialled = 0usize;
    for (peer_id_str, _) in peers {
        let peer_id = PeerId::new(peer_id_str.clone());
        if connected.contains(&peer_id) {
            probes.remove(&peer_id_str);
            continue;
        }
        let probe = probes
            .entry(peer_id_str)
            .or_insert_with(|| ReconnectProbe::new(now));
        if now < probe.next_attempt {
            continue;
        }
        if dialled >= RECONNECT_DIALS_PER_PASS {
            break;
        }
        probe.charge(now);
        dialled += 1;
        redial_replicator(peerstore, transport, &peer_id).await;
    }
}

/// Run one marker-plus-rederive retry pass for any transport.
pub async fn run_retry_pass<S, T>(
    peerstore: &storage::stores::Peerstore<S>,
    transport: &T,
    doc_pusher: &Arc<dyn TransportDocPusher>,
    se_repusher: Option<&Arc<dyn db::merge::SeArtifactRepusher>>,
    force: bool,
) where
    S: storage::corekv::Store + 'static,
    T: P2PTransport,
{
    let peers = match peerstore.get_replicator_retry_peers().await {
        Ok(peers) => peers,
        Err(error) => {
            tracing::debug!(%error, "failed to load retry peers");
            return;
        }
    };

    for (peer_id_str, info_bytes) in peers {
        if let Err(error) = storage::stores::RetryInfo::from_bytes(&info_bytes) {
            tracing::warn!(peer_id = %peer_id_str, %error, "invalid retry info");
            continue;
        }
        let peer_id = PeerId::new(peer_id_str.clone());

        let markers = match peerstore.get_retry_documents(&peer_id_str).await {
            Ok(markers) => markers,
            Err(error) => {
                tracing::debug!(peer_id = %peer_id, %error, "failed to load retry markers");
                continue;
            }
        };
        if markers.is_empty() {
            finish_peer(peerstore, &peer_id_str, true).await;
            continue;
        }
        if !markers.iter().any(|marker| {
            marker.retry_info.is_backpressure_elapsed() && (force || marker.retry_info.is_due())
        }) {
            continue;
        }

        // A failed peer observation is not evidence of disconnection. Keeping
        // the peer's state and letting the replay below charge the configured
        // ladder is the conservative move; treating the observation error as
        // an empty connected set would redial every replicator at once.
        match transport.connected_peers().await {
            Ok(connected) if !connected.contains(&peer_id) => {
                // Connectivity is part of the due retry attempt. Do not create a
                // second two-second redial clock for markers whose ladder has not
                // elapsed yet. `run_reconnect_pass` owns the off-ladder probing.
                redial_replicator(peerstore, transport, &peer_id).await;
                let _ = peerstore.reschedule_retry_peer(&peer_id_str, None, 1).await;
                finish_peer(peerstore, &peer_id_str, false).await;
                continue;
            }
            Ok(_) => {}
            Err(error) => tracing::debug!(
                %peer_id,
                %error,
                "peer observation failed; keeping the existing retry schedule"
            ),
        }

        let deadline = n0_future::time::Instant::now() + MAX_PEER_PASS;
        let mut attempted = 0usize;
        let mut failed = false;
        let mut progressed = false;
        let mut deferred = false;
        let mut report = PassReport::default();
        for marker in &markers {
            // A live push can install a newer peer deadline while this pass
            // awaits another document. The original marker snapshot is stale.
            let current = match peerstore.get_retry_info(&peer_id_str).await {
                Ok(Some(bytes)) => storage::stores::RetryInfo::from_bytes(&bytes).ok(),
                _ => None,
            };
            if current
                .as_ref()
                .is_none_or(|info| !info.is_backpressure_elapsed())
            {
                deferred = true;
                break;
            }
            if !marker.retry_info.is_backpressure_elapsed()
                || (!force && !marker.retry_info.is_due())
            {
                continue;
            }
            if attempted >= MAX_MARKERS_PER_PEER_PASS || n0_future::time::Instant::now() >= deadline
            {
                report.truncated = true;
                break;
            }
            attempted += 1;
            if marker.retry_info.not_before_unix > 0 {
                tracing::debug!(target: "p2p::retry_after", peer_id = %peer_id,
                    retry_not_before_unix = marker.retry_info.not_before_unix,
                    retry_started_unix = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).unwrap_or_default().as_secs(),
                    "Dispatching deferred PushLog retry");
            }
            let replay = async {
                if marker.is_collection_commit() {
                    doc_pusher
                        .retry_collection_commit(&peer_id, &marker.collection_id)
                        .await
                } else {
                    doc_pusher
                        .retry_doc(&peer_id, &marker.doc_id, &marker.collection_id)
                        .await
                }
            };
            match n0_future::time::timeout(REPLAY_TIMEOUT, replay).await {
                Ok(Ok(())) => {
                    progressed = true;
                    // The PushLog acknowledgement already made the marker
                    // transition durable.  SE fan-out is network work and must
                    // never retain the peer's storage-transition writer.
                    if let Some(repusher) = se_repusher {
                        repusher
                            .regenerate_and_push_se_artifacts(&marker.collection_id, &marker.doc_id)
                            .await;
                    }
                }
                Ok(Err(error)) => {
                    let error = error.to_string();
                    report.record(&marker.doc_id, &error);
                    if let Some(delay) = capacity_retry_delay(&error) {
                        // Replay persists typed metadata before returning its legacy
                        // error. Preserve that deadline rather than applying the fallback.
                        let hinted = peerstore
                            .get_retry_info(&peer_id_str)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|bytes| storage::stores::RetryInfo::from_bytes(&bytes).ok())
                            .is_some_and(|info| !info.is_backpressure_elapsed());
                        let delay = if hinted { Duration::ZERO } else { delay };
                        let _ = peerstore
                            .reschedule_retry_peer(&peer_id_str, Some(delay), attempted as u64)
                            .await;
                        // Receiver saturation applies to the peer, not just
                        // this scope.  Rotate the durable cursor and wait for
                        // the paced sweep rather than hammering adjacent docs.
                        deferred = true;
                        break;
                    }
                    failed = true;
                    if is_peer_level_failure(&error) {
                        report.stopped_on = Some("transport unavailable");
                        break;
                    }
                    // Specific to this document: keep its marker, keep going.
                }
                Err(_) => {
                    report.record(&marker.doc_id, "replay timed out");
                    failed = true;
                    report.stopped_on = Some("replay timeout");
                    break;
                }
            }
        }
        report.emit(&peer_id, attempted, markers.len());
        if failed && !deferred {
            // One rung per pass.  A receiver that took documents is healthy:
            // come back at the first interval, not the rung earned while it
            // was unreachable.
            let _ = if progressed {
                peerstore
                    .restart_retry_peer(&peer_id_str, attempted as u64)
                    .await
            } else {
                peerstore
                    .reschedule_retry_peer(&peer_id_str, None, attempted as u64)
                    .await
            };
        }

        let complete = peerstore
            .get_retry_documents(&peer_id_str)
            .await
            .is_ok_and(|markers| markers.is_empty());
        finish_peer(peerstore, &peer_id_str, complete).await;
    }
}

/// One warning per peer per pass. A peer with thousands of markers would
/// otherwise emit one `warn!` per failed marker.
#[derive(Default)]
struct PassReport {
    failures: usize,
    truncated: bool,
    /// Why the peer's pass stopped early, when the peer itself was the reason.
    stopped_on: Option<&'static str>,
    first: Option<(String, String)>,
}

impl PassReport {
    fn record(&mut self, doc_id: &str, error: &str) {
        self.failures += 1;
        self.first
            .get_or_insert_with(|| (doc_id.to_string(), error.to_string()));
    }

    fn emit(&self, peer_id: &PeerId, attempted: usize, due: usize) {
        let Some((doc_id, error)) = self.first.as_ref() else {
            return;
        };
        tracing::warn!(
            %peer_id,
            failures = self.failures,
            attempted,
            due,
            truncated = self.truncated,
            stopped_on = self.stopped_on.unwrap_or("nothing"),
            first_doc_id = %doc_id,
            first_error = %error,
            "retry pass failed to replay markers"
        );
    }
}

async fn finish_peer<S: storage::corekv::Store>(
    peerstore: &storage::stores::Peerstore<S>,
    peer_id: &str,
    complete: bool,
) {
    let status = if complete {
        let _ = peerstore.clear_retry_peer(peer_id).await;
        p2p::ReplicatorStatus::Active
    } else {
        p2p::ReplicatorStatus::Inactive
    };
    let _ = crate::set_persisted_replicator_status(peerstore, peer_id, status).await;
}

/// Run the one durable retry clock used by CLI, embedded, and defra-node.
pub fn spawn_retry_loop<S, T>(
    peerstore: storage::stores::Peerstore<S>,
    transport: T,
    doc_pusher: Arc<dyn TransportDocPusher>,
    se_repusher: Option<Arc<dyn db::merge::SeArtifactRepusher>>,
) -> n0_future::task::JoinHandle<()>
where
    S: storage::corekv::Store + 'static,
    T: P2PTransport,
{
    n0_future::task::spawn(async move {
        if let Err(error) = peerstore.migrate_legacy_push_retries().await {
            tracing::warn!(%error, "failed to migrate legacy push retries after restart");
        }
        let replay = async {
            loop {
                n0_future::time::sleep(p2p::sync::PERSISTED_RETRY_SWEEP_INTERVAL).await;
                run_retry_pass(
                    &peerstore,
                    &transport,
                    &doc_pusher,
                    se_repusher.as_ref(),
                    false,
                )
                .await;
            }
        };
        // Concurrent with the replay sweep rather than sequenced before it: a
        // dial that runs to `RECONNECT_DIAL_TIMEOUT` must not hold up a replay
        // that is already due.
        let reconnect = async {
            let mut probes = RapidHashMap::new();
            loop {
                n0_future::time::sleep(p2p::sync::PERSISTED_RETRY_SWEEP_INTERVAL).await;
                run_reconnect_pass(&peerstore, &transport, &mut probes).await;
            }
        };
        tokio::join!(replay, reconnect);
    })
}

#[cfg(test)]
#[path = "retry_sweep_tests.rs"]
mod sweep_tests;

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use cid::Cid;
    use p2p::message::{
        BranchableSyncReply, BranchableSyncRequest, DocSyncReply, DocSyncRequest, PushLogBroadcast,
        PushLogReply, PushLogRequest, PushSEArtifactsRequest,
    };
    use p2p::topics::DefraTopic;
    use p2p::transport::{MessageId, PeerAddr};
    use p2p::{QueryId, ReplicatorInfo, Result as P2PResult};

    /// Transport double for the reconnect probe. Only `connected_peers` and
    /// `dial` carry behaviour; every other method is unreachable from these
    /// tests.
    #[derive(Clone)]
    struct FakeTransport {
        peer_id: PeerId,
        pubkey: Vec<u8>,
        /// `None` makes `connected_peers` fail, which is the observation
        /// failure the probe has to survive without acting.
        connected: Option<Vec<PeerId>>,
        dial_hangs: bool,
        dials: Arc<kovan::Atom<Vec<PeerId>>>,
        observations: Arc<AtomicUsize>,
    }

    impl FakeTransport {
        fn new(connected: Option<Vec<PeerId>>) -> Self {
            Self {
                peer_id: PeerId::new("local".to_string()),
                pubkey: vec![1],
                connected,
                dial_hangs: false,
                dials: Arc::new(kovan::Atom::new(Vec::new())),
                observations: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn hanging(connected: Option<Vec<PeerId>>) -> Self {
            Self {
                dial_hangs: true,
                ..Self::new(connected)
            }
        }

        fn dialled(&self) -> Vec<PeerId> {
            self.dials.load_clone()
        }
    }
    #[async_trait]
    impl P2PTransport for FakeTransport {
        type ResponseToken = ();

        fn local_peer_id(&self) -> &PeerId {
            &self.peer_id
        }

        fn local_public_key_proto(&self) -> &[u8] {
            &self.pubkey
        }

        fn sign(&self, _data: &[u8]) -> P2PResult<Vec<u8>> {
            Ok(vec![0])
        }

        async fn dial(&self, peer_id: &PeerId, _addrs: Vec<PeerAddr>) -> P2PResult<()> {
            self.dials.rcu(|dials| {
                let mut next = dials.clone();
                next.push(peer_id.clone());
                next
            });
            if self.dial_hangs {
                std::future::pending::<()>().await;
            }
            Ok(())
        }

        async fn disconnect(&self, _peer_id: &PeerId) -> P2PResult<()> {
            Ok(())
        }

        async fn listen(&self, _addr: PeerAddr) -> P2PResult<()> {
            Ok(())
        }

        async fn connected_peers(&self) -> P2PResult<Vec<PeerId>> {
            self.observations.fetch_add(1, Ordering::SeqCst);
            match &self.connected {
                Some(connected) => Ok(connected.clone()),
                None => Err(p2p::error::Error::Transport("observation failed".into())),
            }
        }

        async fn listen_addresses(&self) -> P2PResult<Vec<PeerAddr>> {
            Ok(Vec::new())
        }

        async fn poll_until_connected(
            &self,
            _peer_id: &PeerId,
            _timeout: std::time::Duration,
        ) -> P2PResult<()> {
            Ok(())
        }

        async fn peer_addresses(&self) -> P2PResult<Vec<String>> {
            Ok(Vec::new())
        }

        async fn subscribe(&self, _topic: DefraTopic) -> P2PResult<bool> {
            Ok(true)
        }

        async fn unsubscribe(&self, _topic: DefraTopic) -> P2PResult<bool> {
            Ok(true)
        }

        async fn publish(
            &self,
            _topic: DefraTopic,
            _msg: PushLogBroadcast,
        ) -> P2PResult<MessageId> {
            Ok(MessageId::new("noop".to_string()))
        }

        async fn topic_peers(&self, _topic: DefraTopic) -> P2PResult<Vec<PeerId>> {
            Ok(Vec::new())
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

        async fn send_car_request(&self, _peer_id: &PeerId, _root_cid: Cid) -> P2PResult<()> {
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
            _providers: Vec<PeerId>,
            _missing: Vec<Cid>,
        ) -> P2PResult<QueryId> {
            Ok(QueryId(999))
        }

        async fn cancel_sync(&self, _query_id: QueryId) -> P2PResult<bool> {
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

    fn in_memory_peerstore() -> storage::stores::Peerstore<storage::backends::RegolithStore> {
        storage::stores::Peerstore::new(Arc::new(
            storage::backends::RegolithStore::in_memory().unwrap(),
        ))
    }

    /// One replicator peer carrying a pending document marker, which is what
    /// puts it in `get_replicator_retry_peers`.
    async fn seed_retry_peer(
        peerstore: &storage::stores::Peerstore<storage::backends::RegolithStore>,
        peer_id: &str,
        address: &str,
    ) {
        let replicator = ReplicatorInfo::from_raw(
            peer_id.to_string(),
            vec!["collection-a".to_string()],
            vec![address.to_string()],
        );
        peerstore
            .create_replicator(peer_id, &replicator.to_bytes().unwrap())
            .await
            .unwrap();
        peerstore
            .record_push_failure(
                peer_id,
                "doc-a",
                "collection-a",
                &storage::stores::RetryInfo::new_initial()
                    .to_bytes()
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    async fn next_retry_unix(
        peerstore: &storage::stores::Peerstore<storage::backends::RegolithStore>,
        peer_id: &str,
    ) -> u64 {
        let bytes = peerstore.get_retry_info(peer_id).await.unwrap().unwrap();
        storage::stores::RetryInfo::from_bytes(&bytes)
            .unwrap()
            .next_retry_unix
    }

    #[tokio::test(start_paused = true)]
    async fn observation_failure_leaves_every_peer_alone() {
        let peerstore = in_memory_peerstore();
        for index in 0..4 {
            seed_retry_peer(&peerstore, &format!("peer-{index}"), "/memory/1").await;
        }
        let transport = FakeTransport::new(None);
        let mut probes = RapidHashMap::new();

        run_reconnect_pass(&peerstore, &transport, &mut probes).await;

        assert_eq!(transport.observations.load(Ordering::SeqCst), 1);
        assert!(
            transport.dialled().is_empty(),
            "a failed observation must not be read as a disconnected fleet"
        );
        assert!(
            probes.is_empty(),
            "no probe state is invented from an error"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn reconnect_probe_leaves_the_configured_retry_deadline_alone() {
        let peerstore = in_memory_peerstore();
        seed_retry_peer(&peerstore, "peer-a", "/memory/1").await;
        // Stand in for a configured second rung far out on the ladder.
        peerstore
            .reschedule_retry_peer("peer-a", Some(std::time::Duration::from_secs(3600)), 0)
            .await
            .unwrap();
        let before = next_retry_unix(&peerstore, "peer-a").await;

        let transport = FakeTransport::new(Some(Vec::new()));
        let mut probes = RapidHashMap::new();
        run_reconnect_pass(&peerstore, &transport, &mut probes).await;

        assert_eq!(transport.dialled().len(), 1, "the peer is still probed");
        assert_eq!(
            next_retry_unix(&peerstore, "peer-a").await,
            before,
            "the probe must not rewrite the configured replay deadline"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_hung_dial_is_bounded_and_does_not_strand_later_peers() {
        let peerstore = in_memory_peerstore();
        seed_retry_peer(&peerstore, "peer-a", "/memory/1").await;
        seed_retry_peer(&peerstore, "peer-b", "/memory/2").await;
        let transport = FakeTransport::hanging(Some(Vec::new()));
        let mut probes = RapidHashMap::new();

        let start = n0_future::time::Instant::now();
        run_reconnect_pass(&peerstore, &transport, &mut probes).await;

        assert_eq!(
            transport.dialled().len(),
            2,
            "a dial that hangs must not strand the peers queued behind it"
        );
        assert_eq!(
            n0_future::time::Instant::now() - start,
            RECONNECT_DIAL_TIMEOUT * 2
        );
    }

    #[tokio::test(start_paused = true)]
    async fn many_disconnected_peers_cost_a_bounded_number_of_dials_per_pass() {
        let peerstore = in_memory_peerstore();
        let peers = RECONNECT_DIALS_PER_PASS * 5;
        for index in 0..peers {
            seed_retry_peer(&peerstore, &format!("peer-{index:02}"), "/memory/1").await;
        }
        let transport = FakeTransport::new(Some(Vec::new()));
        let mut probes = RapidHashMap::new();

        // Each pass takes a bounded bite, and consecutive passes walk the rest
        // of the fleet rather than re-dialling the peers already probed.
        for pass in 1..=5 {
            run_reconnect_pass(&peerstore, &transport, &mut probes).await;
            assert_eq!(transport.dialled().len(), RECONNECT_DIALS_PER_PASS * pass);
        }
        run_reconnect_pass(&peerstore, &transport, &mut probes).await;
        assert_eq!(
            transport.dialled().len(),
            peers,
            "once every peer is charged, a pass costs no dials at all"
        );
        assert_eq!(probes.len(), peers);
    }

    #[tokio::test(start_paused = true)]
    async fn per_peer_backoff_doubles_and_clears_on_reconnect() {
        let peerstore = in_memory_peerstore();
        seed_retry_peer(&peerstore, "peer-a", "/memory/1").await;
        let disconnected = FakeTransport::new(Some(Vec::new()));
        let mut probes = RapidHashMap::new();

        run_reconnect_pass(&peerstore, &disconnected, &mut probes).await;
        assert_eq!(disconnected.dialled().len(), 1);
        assert_eq!(probes["peer-a"].backoff, RECONNECT_BACKOFF_MIN * 2);

        run_reconnect_pass(&peerstore, &disconnected, &mut probes).await;
        assert_eq!(disconnected.dialled().len(), 1, "still inside the backoff");

        tokio::time::advance(RECONNECT_BACKOFF_MIN).await;
        run_reconnect_pass(&peerstore, &disconnected, &mut probes).await;
        assert_eq!(disconnected.dialled().len(), 2);
        assert_eq!(probes["peer-a"].backoff, RECONNECT_BACKOFF_MIN * 4);

        // `PeerConnected` is what activates the durable markers. Once the peer
        // is back, the probe's only job is to forget its backoff.
        let connected = FakeTransport::new(Some(vec![PeerId::new("peer-a".to_string())]));
        run_reconnect_pass(&peerstore, &connected, &mut probes).await;
        assert!(connected.dialled().is_empty());
        assert!(probes.is_empty());
    }

    #[cfg(feature = "iroh")]
    mod iroh_probe {
        use std::net::{IpAddr, Ipv4Addr};

        use p2p::iroh::{
            load_or_generate_secret_key, spawn_endpoint, IrohDiscoveryConfig, IrohEndpointConfig,
            IrohRelayModeConfig, IrohTransport,
        };

        use super::*;

        #[tokio::test]
        async fn a_stopped_iroh_endpoint_reads_as_observation_failure() {
            let peerstore = in_memory_peerstore();
            for index in 0..4 {
                seed_retry_peer(&peerstore, &format!("peer-{index}"), "addr").await;
            }
            let key = load_or_generate_secret_key(None).await.expect("key");
            let (command_tx, command_rx) = tokio::sync::mpsc::channel(4);
            // A dropped receiver is what a stopped endpoint task looks like
            // from the transport: every command fails.
            drop(command_rx);
            let transport = IrohTransport::new(command_tx, key);

            let mut probes = RapidHashMap::new();
            run_reconnect_pass(&peerstore, &transport, &mut probes).await;

            assert!(
                probes.is_empty(),
                "a stopped iroh endpoint is not a disconnected fleet"
            );
        }

        #[tokio::test]
        async fn an_unreachable_iroh_peer_is_probed_off_the_replay_ladder() {
            let peerstore = in_memory_peerstore();
            let key = load_or_generate_secret_key(None).await.expect("local key");
            let remote = load_or_generate_secret_key(None).await.expect("remote key");
            let remote_id = remote.public().to_string();
            // Port 1 on loopback: parses as an iroh dial address, answers
            // nothing.
            seed_retry_peer(&peerstore, &remote_id, &format!("{remote_id}@127.0.0.1:1")).await;
            let before = next_retry_unix(&peerstore, &remote_id).await;

            let (command_tx, _events, _replicators, _task) = spawn_endpoint(IrohEndpointConfig {
                secret_key: key.clone(),
                node_identity: None,
                relay_mode: IrohRelayModeConfig::Disabled,
                discovery: IrohDiscoveryConfig::Disabled,
                bind_port: None,
                bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                max_concurrent_multipath_paths: None,
                gossip_heal: Default::default(),
                allowlist: Default::default(),
            })
            .await
            .expect("endpoint");
            let transport = IrohTransport::new(command_tx, key);

            let mut probes = RapidHashMap::new();
            run_reconnect_pass(&peerstore, &transport, &mut probes).await;

            assert_eq!(
                probes[remote_id.as_str()].backoff,
                RECONNECT_BACKOFF_MIN * 2,
                "the probe fired and charged its own backoff"
            );
            assert_eq!(
                next_retry_unix(&peerstore, &remote_id).await,
                before,
                "the iroh probe must not rewrite the configured replay deadline"
            );
        }
    }

    #[test]
    fn capacity_nack_uses_the_paced_sweep_without_becoming_a_push_failure() {
        let error = "peer rejected replay after 0 successful block(s): at capacity: receiver is saturated, back off";
        assert_eq!(
            capacity_retry_delay(error),
            Some(p2p::sync::PERSISTED_RETRY_SWEEP_INTERVAL)
        );
        assert_eq!(capacity_retry_delay("connection reset"), None);
    }

    #[test]
    fn backpressure_is_classified_by_exact_sentinel_not_substring() {
        for sentinel in [
            p2p::error::AT_CAPACITY_MESSAGE,
            p2p::error::RATE_LIMITED_MESSAGE,
        ] {
            assert_eq!(
                capacity_retry_delay(sentinel),
                Some(p2p::sync::PERSISTED_RETRY_SWEEP_INTERVAL),
                "bare sentinel {sentinel:?} was not read as backpressure"
            );
            assert_eq!(
                capacity_retry_delay(&format!("peer rejected replay: {sentinel}")),
                Some(p2p::sync::PERSISTED_RETRY_SWEEP_INTERVAL),
                "wrapped sentinel {sentinel:?} was not read as backpressure"
            );
        }
        // A document whose own error text merely mentions the phrase is a
        // document failure, not a peer-wide wait.
        assert_eq!(
            capacity_retry_delay("replay push failed: field \"rate limited\" is unknown"),
            None
        );
        assert_eq!(capacity_retry_delay("rate limited by the operator"), None);
    }

    #[test]
    fn only_the_transport_unavailable_prefix_stops_the_peer_pass() {
        assert!(is_peer_level_failure(
            "transport became unavailable after 0 successful block(s): connection closed"
        ));
        assert!(!is_peer_level_failure(
            "replay push failed after 0 successful block(s): doc is not permitted"
        ));
    }

    fn failure(acknowledged: bool) -> p2p::sync::PushFailure {
        p2p::sync::PushFailure {
            peer_id: "peer-a".to_string(),
            doc_id: "doc-a".to_string(),
            collection_id: "collection-a".to_string(),
            cid: "head-a".to_string(),
            head_priority: 7,
            create_retry: false,
            acknowledged,
            durable_tx: None,
            retry_after: None,
            admission_only: false,
        }
    }

    #[tokio::test]
    async fn shared_failure_recorder_registers_then_clears_current_scope() {
        let store = Arc::new(storage::backends::RegolithStore::in_memory().unwrap());
        let peerstore = storage::stores::Peerstore::new(Arc::clone(&store));
        let replicator = p2p::ReplicatorInfo::from_raw(
            "peer-a".to_string(),
            vec!["collection-a".to_string()],
            Vec::new(),
        );
        peerstore
            .create_replicator("peer-a", &replicator.to_bytes().unwrap())
            .await
            .unwrap();

        let (tx, rx) = tokio::sync::mpsc::channel(2);
        let task = spawn_failure_recorder(storage::stores::Peerstore::new(Arc::clone(&store)), rx);

        let (registered_tx, registered_rx) = tokio::sync::oneshot::channel();
        let mut announced = failure(false);
        announced.durable_tx = Some(registered_tx);
        tx.send(announced).await.unwrap();
        assert!(registered_rx.await.unwrap());
        assert_eq!(
            peerstore.get_retry_documents("peer-a").await.unwrap().len(),
            1
        );

        let (cleared_tx, cleared_rx) = tokio::sync::oneshot::channel();
        let mut acknowledged = failure(true);
        acknowledged.durable_tx = Some(cleared_tx);
        tx.send(acknowledged).await.unwrap();
        assert!(cleared_rx.await.unwrap());
        assert!(peerstore
            .get_retry_documents("peer-a")
            .await
            .unwrap()
            .is_empty());

        drop(tx);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn retry_after_live_admission_reads_persisted_state_after_recorder_restart() {
        let store = Arc::new(storage::backends::RegolithStore::in_memory().unwrap());
        let peerstore = storage::stores::Peerstore::new(store.clone());
        let replicator =
            p2p::ReplicatorInfo::from_raw("peer-a".into(), vec!["collection-a".into()], Vec::new());
        peerstore
            .create_replicator("peer-a", &replicator.to_bytes().unwrap())
            .await
            .unwrap();
        peerstore
            .observe_push_head("peer-a", "doc-a", "collection-a")
            .await
            .unwrap();
        peerstore
            .reschedule_retry_peer("peer-a", Some(Duration::from_secs(45)), 0)
            .await
            .unwrap();
        for _ in 0..2 {
            let (tx, rx) = tokio::sync::mpsc::channel(2);
            let recorder =
                spawn_failure_recorder(storage::stores::Peerstore::new(store.clone()), rx);
            let (response, allowed) = tokio::sync::oneshot::channel();
            let mut check = failure(false);
            check.admission_only = true;
            check.durable_tx = Some(response);
            tx.send(check).await.unwrap();
            assert!(!allowed.await.unwrap());
            drop(tx);
            recorder.await.unwrap();
        }
        assert_eq!(
            peerstore.get_retry_documents("peer-a").await.unwrap().len(),
            1
        );
    }
}
