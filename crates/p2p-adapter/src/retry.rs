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

/// The running failure recorder: background work of the worker pool, or in
/// a browser a task of the page's event loop.
#[cfg(not(target_arch = "wasm32"))]
pub type FailureRecorder = workers::Background;
/// The running failure recorder: background work of the worker pool, or in
/// a browser a task of the page's event loop.
#[cfg(target_arch = "wasm32")]
pub type FailureRecorder = n0_future::task::JoinHandle<()>;

/// Record head announcements and compare-delete only the exact durable marker
/// generation returned to the sender before its PushLog attempt. Every write
/// it makes is storage work: it runs as background work of the pool the
/// peerstore is placed on (the fallback pool without one), until its
/// channel closes or it is aborted.
pub fn spawn_failure_recorder<S: storage::corekv::Store + 'static>(
    peerstore: storage::stores::Peerstore<S>,
    mut failures: p2p::rt::mpsc::Receiver<p2p::sync::PushFailure>,
) -> FailureRecorder {
    #[cfg(not(target_arch = "wasm32"))]
    let pool = peerstore.pool().cloned();
    let record = async move {
        while let Some(failure) = failures.recv().await {
            record_failure(&peerstore, failure).await;
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    return workers::spawn_background(pool.as_ref(), "push failure recorder", move || record);
    // The page's thread is a browser's only one, and its event loop the
    // only runtime.
    #[cfg(target_arch = "wasm32")]
    n0_future::task::spawn(record)
}

/// Records one push failure on the peerstore: its marker, the generation its
/// sender waits for, and the replicator's status.
async fn record_failure<S: storage::corekv::Store>(
    peerstore: &storage::stores::Peerstore<S>,
    mut failure: p2p::sync::PushFailure,
) {
    let durable_tx = failure.durable_tx.take();

    if failure.admission_only {
        let generation = match failure.marker_generation {
            Some(expected_generation) => match peerstore
                .admit_push_scope(
                    &failure.peer_id,
                    &failure.doc_id,
                    &failure.collection_id,
                    expected_generation,
                )
                .await
            {
                Ok(true) => Some(expected_generation),
                Ok(false) => None,
                Err(error) => {
                    tracing::warn!(%error, "failed to validate live push admission marker");
                    None
                }
            },
            None => None,
        };
        let _ = durable_tx.map(|tx| tx.send(generation));
        return;
    }

    let result: storage::corekv::Result<Option<u64>> = if failure.acknowledged {
        let acked = std::slice::from_ref(&failure.cid);
        let is_collection = failure.doc_id.is_empty();
        match failure.marker_generation {
            Some(generation) => peerstore
                .complete_retry_scope(
                    &failure.peer_id,
                    &failure.doc_id,
                    &failure.collection_id,
                    is_collection,
                    generation,
                    acked,
                )
                .await
                .map(|cleared| cleared.then_some(generation)),
            // A live push that registered no marker still records what the
            // peer acknowledged, as main does.
            None => peerstore
                .record_acknowledged_heads(
                    &failure.peer_id,
                    &failure.doc_id,
                    &failure.collection_id,
                    is_collection,
                    acked,
                )
                .await
                .map(|_| None),
        }
    } else if failure.create_retry {
        let mut info = storage::stores::RetryInfo::new_initial();
        if let Some(delay) = failure.retry_after {
            info.defer_for_hint(delay);
            tracing::debug!(target: "p2p::retry_after", peer_id = %failure.peer_id,
                retry_after_ms = delay.as_millis() as u64,
                retry_not_before_unix = info.not_before_unix,
                "Persisting negotiated PushLog retry-after");
        }
        let info_bytes = match info.to_bytes() {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!(%error, "failed to serialize retry info");
                let _ = durable_tx.map(|tx| tx.send(None));
                return;
            }
        };
        match failure.marker_generation {
            Some(expected_generation) => {
                peerstore
                    .refresh_push_failure_with_generation(
                        &failure.peer_id,
                        &failure.doc_id,
                        &failure.collection_id,
                        expected_generation,
                        &info_bytes,
                    )
                    .await
            }
            // SE-artifact failures have no admitted PushJob or head
            // marker. They intentionally create the initial retry
            // obligation; terminal PushJob failures always carry the
            // generation returned by the admission handshake.
            None if failure.initial_registration => {
                peerstore
                    .record_push_failure_with_generation(
                        &failure.peer_id,
                        &failure.doc_id,
                        &failure.collection_id,
                        &info_bytes,
                    )
                    .await
            }
            None => Ok(None),
        }
    } else {
        peerstore
            .observe_push_head_with_generation(
                &failure.peer_id,
                &failure.doc_id,
                &failure.collection_id,
            )
            .await
    };
    let marker_generation = match result {
        Ok(generation) => generation,
        Err(error) => {
            tracing::warn!(%error, "failed to record push failure");
            let _ = durable_tx.map(|tx| tx.send(None));
            return;
        }
    };

    let _ = durable_tx.map(|tx| tx.send(marker_generation));
    if !failure.acknowledged && failure.create_retry {
        if let Some(generation) = marker_generation {
            if let Err(error) = crate::set_persisted_replicator_status_for_scope_generation(
                peerstore,
                &failure.peer_id,
                &failure.doc_id,
                &failure.collection_id,
                generation,
                p2p::ReplicatorStatus::Inactive,
            )
            .await
            {
                tracing::warn!(%error, "failed to mark replicator inactive");
            }
        }
    }
}

/// Activate a dormant durable peer schedule after either transport reconnects,
/// on the pool `peerstore` is placed on.
pub async fn activate_retry_peer<S>(peerstore: &storage::stores::Peerstore<S>, peer_id: &PeerId)
where
    S: storage::corekv::Store + 'static,
{
    let peer = peer_id.as_str().to_owned();
    let activated = peerstore
        .run(move |peerstore| async move { peerstore.activate_retry_peer(&peer).await })
        .await;
    match activated {
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
    S: storage::corekv::Store + 'static,
    T: P2PTransport,
{
    let peer = peer_id.as_str().to_owned();
    let replicator = peerstore
        .run(move |peerstore| async move { peerstore.get_replicator(&peer).await })
        .await;
    let Ok(Some(bytes)) = replicator else {
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
    match p2p::rt::time::timeout(RECONNECT_DIAL_TIMEOUT, transport.dial(peer_id, addrs)).await {
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
    next_attempt: p2p::rt::time::Instant,
    backoff: std::time::Duration,
}

impl ReconnectProbe {
    fn new(now: p2p::rt::time::Instant) -> Self {
        Self {
            next_attempt: now,
            backoff: RECONNECT_BACKOFF_MIN,
        }
    }

    /// Charge the backoff *before* dialling, so a dial that hangs to its
    /// timeout still defers the next probe instead of retrying immediately.
    fn charge(&mut self, now: p2p::rt::time::Instant) {
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
    S: storage::corekv::Store + 'static,
    T: P2PTransport,
{
    let Ok(peers) = retry_peers(peerstore).await else {
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

    let now = p2p::rt::time::Instant::now();
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
    if let Err(error) = doc_pusher.recover_replicator_installs().await {
        tracing::warn!(%error, "failed to recover interrupted replicator installs");
    }
    let peers = match retry_peers(peerstore).await {
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
        let peer = peer_id_str.clone();
        let incarnation = peerstore
            .run(move |peerstore| async move { peerstore.get_replicator_incarnation(&peer).await })
            .await;
        let incarnation = match incarnation {
            Ok(Some(incarnation)) => incarnation,
            Ok(None) => continue,
            Err(error) => {
                tracing::debug!(peer_id = %peer_id_str, %error, "failed to load peer incarnation");
                continue;
            }
        };
        let peer_id = PeerId::new(peer_id_str.clone());

        let markers = match retry_documents(peerstore, &peer_id_str).await {
            Ok(markers) => markers,
            Err(error) => {
                tracing::debug!(peer_id = %peer_id, %error, "failed to load retry markers");
                continue;
            }
        };
        if markers.is_empty() {
            finish_peer(peerstore, &peer_id_str, incarnation, true).await;
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
                let _ = reschedule(peerstore, &peer_id_str, incarnation, None, 1).await;
                finish_peer(peerstore, &peer_id_str, incarnation, false).await;
                continue;
            }
            Ok(_) => {}
            Err(error) => tracing::debug!(
                %peer_id,
                %error,
                "peer observation failed; keeping the existing retry schedule"
            ),
        }

        let deadline = p2p::rt::time::Instant::now() + MAX_PEER_PASS;
        let mut attempted = 0usize;
        let mut failed = false;
        let mut progressed = false;
        let mut deferred = false;
        let mut report = PassReport::default();
        for marker in &markers {
            // A live push can install a newer peer deadline while this pass
            // awaits another document. The original marker snapshot is stale.
            let current = match retry_info(peerstore, &peer_id_str).await {
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
            if attempted >= MAX_MARKERS_PER_PEER_PASS || p2p::rt::time::Instant::now() >= deadline {
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
                        .retry_collection_commit(&peer_id, &marker.collection_id, marker.generation)
                        .await
                } else {
                    doc_pusher
                        .retry_doc(
                            &peer_id,
                            &marker.doc_id,
                            &marker.collection_id,
                            marker.generation,
                        )
                        .await
                }
            };
            match p2p::rt::time::timeout(REPLAY_TIMEOUT, replay).await {
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
                        let hinted = retry_info(peerstore, &peer_id_str)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|bytes| storage::stores::RetryInfo::from_bytes(&bytes).ok())
                            .is_some_and(|info| !info.is_backpressure_elapsed());
                        let delay = if hinted { Duration::ZERO } else { delay };
                        let attempts = attempted as u64;
                        let _ =
                            reschedule(peerstore, &peer_id_str, incarnation, Some(delay), attempts)
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
                let peer = peer_id_str.clone();
                let attempts = attempted as u64;
                peerstore
                    .run(move |peerstore| async move {
                        peerstore
                            .restart_retry_peer_for_incarnation(&peer, incarnation, attempts)
                            .await
                    })
                    .await
            } else {
                reschedule(peerstore, &peer_id_str, incarnation, None, attempted as u64).await
            };
        }

        let complete = retry_documents(peerstore, &peer_id_str)
            .await
            .is_ok_and(|markers| markers.is_empty());
        finish_peer(peerstore, &peer_id_str, incarnation, complete).await;
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

/// The peers with a durable retry schedule.
async fn retry_peers<S: storage::corekv::Store + 'static>(
    peerstore: &storage::stores::Peerstore<S>,
) -> storage::corekv::Result<Vec<(String, bytes::Bytes)>> {
    peerstore
        .run(|peerstore| async move { peerstore.get_replicator_retry_peers().await })
        .await
}

/// `peer`'s retry markers.
async fn retry_documents<S: storage::corekv::Store + 'static>(
    peerstore: &storage::stores::Peerstore<S>,
    peer: &str,
) -> storage::corekv::Result<Vec<storage::stores::PushRetryMarker>> {
    let peer = peer.to_owned();
    peerstore
        .run(move |peerstore| async move { peerstore.get_retry_documents(&peer).await })
        .await
}

/// `peer`'s retry schedule.
async fn retry_info<S: storage::corekv::Store + 'static>(
    peerstore: &storage::stores::Peerstore<S>,
    peer: &str,
) -> storage::corekv::Result<Option<bytes::Bytes>> {
    let peer = peer.to_owned();
    peerstore
        .run(move |peerstore| async move { peerstore.get_retry_info(&peer).await })
        .await
}

/// Moves `peer`'s schedule on by `attempts` rungs, or to `delay`.
async fn reschedule<S: storage::corekv::Store + 'static>(
    peerstore: &storage::stores::Peerstore<S>,
    peer: &str,
    incarnation: u64,
    delay: Option<Duration>,
    attempts: u64,
) -> storage::corekv::Result<bool> {
    let peer = peer.to_owned();
    peerstore
        .run(move |peerstore| async move {
            peerstore
                .reschedule_retry_peer_for_incarnation(&peer, incarnation, delay, attempts)
                .await
        })
        .await
}

/// Ends `peer`'s pass: clears its schedule when it is complete, and records
/// its status.
async fn finish_peer<S: storage::corekv::Store + 'static>(
    peerstore: &storage::stores::Peerstore<S>,
    peer_id: &str,
    incarnation: u64,
    complete: bool,
) {
    let peer = peer_id.to_owned();
    let _ = peerstore
        .run(move |peerstore| async move {
            record_peer_finished(&peerstore, &peer, incarnation, complete).await;
            Ok(())
        })
        .await;
}

async fn record_peer_finished<S: storage::corekv::Store>(
    peerstore: &storage::stores::Peerstore<S>,
    peer_id: &str,
    incarnation: u64,
    complete: bool,
) {
    let status = if complete {
        let _ = peerstore
            .clear_retry_peer_for_incarnation(peer_id, incarnation)
            .await;
        p2p::ReplicatorStatus::Active
    } else {
        p2p::ReplicatorStatus::Inactive
    };
    let _ = crate::set_persisted_replicator_status_for_incarnation_with_marker_state(
        peerstore,
        peer_id,
        incarnation,
        status,
        complete,
    )
    .await;
}

/// Run the one durable retry clock used by CLI, embedded, and defra-node, on
/// the network edge `edge`: its timers and transport waits run there, its
/// storage calls on the pool the peerstore is placed on.
pub fn spawn_retry_loop<S, T>(
    peerstore: storage::stores::Peerstore<S>,
    transport: T,
    doc_pusher: Arc<dyn TransportDocPusher>,
    se_repusher: Option<Arc<dyn db::merge::SeArtifactRepusher>>,
    edge: &edge::Edge,
) -> edge::Task<()>
where
    S: storage::corekv::Store + 'static,
    T: P2PTransport,
{
    let clock = edge.clone();
    let sweeps = async move {
        let replay = async {
            loop {
                clock.sleep(p2p::sync::PERSISTED_RETRY_SWEEP_INTERVAL).await;
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
                clock.sleep(p2p::sync::PERSISTED_RETRY_SWEEP_INTERVAL).await;
                run_reconnect_pass(&peerstore, &transport, &mut probes).await;
            }
        };
        futures_lite::future::zip(replay, reconnect).await;
    };
    // A stopping edge ends its sleeps early: the sweeps stop with it.
    let closing = edge.closing();
    edge.spawn(async move {
        p2p::rt::select! {
            () = sweeps => {}
            () = closing => {}
        }
    })
}

#[cfg(test)]
#[path = "retry_sweep_tests.rs"]
mod sweep_tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "retry_pool_tests.rs"]
mod pool_tests;

#[cfg(test)]
#[path = "../tests/unit/retry_hints.rs"]
mod hint_tests;

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

    /// Transport double for reconnect probes and receiver backpressure.
    #[derive(Clone)]
    pub(super) struct FakeTransport {
        peer_id: PeerId,
        pubkey: Vec<u8>,
        /// `None` makes `connected_peers` fail, which is the observation
        /// failure the probe has to survive without acting.
        connected: Option<Vec<PeerId>>,
        dial_hangs: bool,
        dials: Arc<kovan::Atom<Vec<PeerId>>>,
        observations: Arc<AtomicUsize>,
        reply: Option<PushLogReply>,
        pub(super) sends: Arc<AtomicUsize>,
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
                reply: None,
                sends: Arc::new(AtomicUsize::new(0)),
            }
        }

        pub(super) fn with_hint(peer: PeerId) -> Self {
            let mut reply = PushLogReply::error("hint", p2p::error::RATE_LIMITED_MESSAGE);
            reply.retry_after_ms = Some(45_000);
            Self {
                reply: Some(reply),
                ..Self::new(Some(vec![peer]))
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
            self.sends.fetch_add(1, Ordering::SeqCst);
            Ok(self
                .reply
                .clone()
                .unwrap_or_else(|| PushLogReply::success("noop")))
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

    #[nuclei::test(start_paused = true)]
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

    #[nuclei::test(start_paused = true)]
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

    #[nuclei::test(start_paused = true)]
    async fn a_hung_dial_is_bounded_and_does_not_strand_later_peers() {
        let peerstore = in_memory_peerstore();
        seed_retry_peer(&peerstore, "peer-a", "/memory/1").await;
        seed_retry_peer(&peerstore, "peer-b", "/memory/2").await;
        let transport = FakeTransport::hanging(Some(Vec::new()));
        let mut probes = RapidHashMap::new();

        let start = p2p::rt::time::Instant::now();
        run_reconnect_pass(&peerstore, &transport, &mut probes).await;

        assert_eq!(
            transport.dialled().len(),
            2,
            "a dial that hangs must not strand the peers queued behind it"
        );
        assert_eq!(
            p2p::rt::time::Instant::now() - start,
            RECONNECT_DIAL_TIMEOUT * 2
        );
    }

    #[nuclei::test(start_paused = true)]
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

    #[nuclei::test(start_paused = true)]
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

        nuclei::time::advance(RECONNECT_BACKOFF_MIN).await;
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

        #[nuclei::test]
        async fn a_stopped_iroh_endpoint_reads_as_observation_failure() {
            let peerstore = in_memory_peerstore();
            for index in 0..4 {
                seed_retry_peer(&peerstore, &format!("peer-{index}"), "addr").await;
            }
            let key = load_or_generate_secret_key(None).await.expect("key");
            let (command_tx, command_rx) = p2p::rt::mpsc::channel(4);
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

        #[nuclei::test]
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
            initial_registration: false,
            acknowledged,
            marker_generation: None,
            durable_tx: None,
            retry_after: None,
            admission_only: false,
        }
    }

    async fn check_live_admission(
        tx: &p2p::rt::mpsc::Sender<p2p::sync::PushFailure>,
        marker_generation: u64,
    ) -> Option<u64> {
        let (durable_tx, durable_rx) = p2p::rt::oneshot::channel();
        let mut check = failure(false);
        check.admission_only = true;
        check.marker_generation = Some(marker_generation);
        check.durable_tx = Some(durable_tx);
        tx.send(check).await.unwrap();
        durable_rx.await.unwrap()
    }

    async fn record_terminal_failure(
        tx: &p2p::rt::mpsc::Sender<p2p::sync::PushFailure>,
        marker_generation: u64,
        retry_after: Option<Duration>,
    ) -> Option<u64> {
        let (durable_tx, durable_rx) = p2p::rt::oneshot::channel();
        let mut failed = failure(false);
        failed.create_retry = true;
        failed.marker_generation = Some(marker_generation);
        failed.retry_after = retry_after;
        failed.durable_tx = Some(durable_tx);
        tx.send(failed).await.unwrap();
        durable_rx.await.unwrap()
    }

    async fn record_initial_failure(
        tx: &p2p::rt::mpsc::Sender<p2p::sync::PushFailure>,
        initial_registration: bool,
    ) -> Option<u64> {
        let (durable_tx, durable_rx) = p2p::rt::oneshot::channel();
        let mut failed = failure(false);
        failed.create_retry = true;
        failed.initial_registration = initial_registration;
        failed.durable_tx = Some(durable_tx);
        tx.send(failed).await.unwrap();
        durable_rx.await.unwrap()
    }

    #[nuclei::test]
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

        let (tx, rx) = p2p::rt::mpsc::channel(2);
        let task = spawn_failure_recorder(storage::stores::Peerstore::new(Arc::clone(&store)), rx);

        let (registered_tx, registered_rx) = p2p::rt::oneshot::channel();
        let mut announced = failure(false);
        announced.durable_tx = Some(registered_tx);
        tx.send(announced).await.unwrap();
        let generation = registered_rx.await.unwrap().unwrap();
        assert_eq!(
            peerstore.get_retry_documents("peer-a").await.unwrap().len(),
            1
        );

        let (cleared_tx, cleared_rx) = p2p::rt::oneshot::channel();
        let mut acknowledged = failure(true);
        acknowledged.marker_generation = Some(generation);
        acknowledged.durable_tx = Some(cleared_tx);
        tx.send(acknowledged).await.unwrap();
        assert_eq!(cleared_rx.await.unwrap(), Some(generation));
        assert!(peerstore
            .get_retry_documents("peer-a")
            .await
            .unwrap()
            .is_empty());

        drop(tx);
        task.await.unwrap();
    }

    #[nuclei::test]
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
            .record_retry_after("peer-a", Duration::from_secs(45))
            .await
            .unwrap();
        for _ in 0..2 {
            let (tx, rx) = p2p::rt::mpsc::channel(2);
            let recorder =
                spawn_failure_recorder(storage::stores::Peerstore::new(store.clone()), rx);
            let (response, allowed) = p2p::rt::oneshot::channel();
            let mut check = failure(false);
            check.admission_only = true;
            check.durable_tx = Some(response);
            tx.send(check).await.unwrap();
            assert!(allowed.await.unwrap().is_none());
            drop(tx);
            recorder.await.unwrap();
        }
        assert_eq!(
            peerstore.get_retry_documents("peer-a").await.unwrap().len(),
            1
        );
    }

    #[nuclei::test]
    async fn live_admission_requires_current_peer_scope_generation_and_elapsed_deadline() {
        let store = Arc::new(storage::backends::RegolithStore::in_memory().unwrap());
        let peerstore = storage::stores::Peerstore::new(store.clone());
        let replicator =
            p2p::ReplicatorInfo::from_raw("peer-a".into(), vec!["collection-a".into()], Vec::new());
        peerstore
            .create_replicator("peer-a", &replicator.to_bytes().unwrap())
            .await
            .unwrap();
        let old_generation = peerstore
            .observe_push_head_with_generation("peer-a", "doc-a", "collection-a")
            .await
            .unwrap()
            .unwrap();

        let (tx, rx) = p2p::rt::mpsc::channel(8);
        let recorder = spawn_failure_recorder(storage::stores::Peerstore::new(store.clone()), rx);

        // A normal first push is admitted only while its exact marker is live.
        assert_eq!(
            check_live_admission(&tx, old_generation).await,
            Some(old_generation)
        );

        // A live marker does not bypass the durable peer backoff deadline.
        peerstore
            .record_retry_after("peer-a", Duration::from_secs(45))
            .await
            .unwrap();
        assert_eq!(check_live_admission(&tx, old_generation).await, None);

        // Forget removes both the peer and its marker. A job queued before the
        // forget is rejected when it reaches the admission linearization point.
        peerstore.delete_replicator("peer-a").await.unwrap();
        assert_eq!(check_live_admission(&tx, old_generation).await, None);

        // Re-adding the peer creates a fresh generation. It admits the new job,
        // while the queued old job remains fenced out.
        peerstore
            .create_replicator("peer-a", &replicator.to_bytes().unwrap())
            .await
            .unwrap();
        let new_generation = peerstore
            .observe_push_head_with_generation("peer-a", "doc-a", "collection-a")
            .await
            .unwrap()
            .unwrap();
        assert!(new_generation > old_generation);
        assert_eq!(check_live_admission(&tx, old_generation).await, None);
        assert_eq!(
            check_live_admission(&tx, new_generation).await,
            Some(new_generation)
        );

        drop(tx);
        recorder.await.unwrap();
    }

    #[nuclei::test]
    async fn terminal_failure_refresh_is_cas_fenced_by_source_generation() {
        let store = Arc::new(storage::backends::RegolithStore::in_memory().unwrap());
        let peerstore = storage::stores::Peerstore::new(store.clone());
        let replicator =
            p2p::ReplicatorInfo::from_raw("peer-a".into(), vec!["collection-a".into()], Vec::new());
        peerstore
            .create_replicator("peer-a", &replicator.to_bytes().unwrap())
            .await
            .unwrap();
        let admitted_generation = peerstore
            .observe_push_head_with_generation("peer-a", "doc-a", "collection-a")
            .await
            .unwrap()
            .unwrap();

        let (tx, rx) = p2p::rt::mpsc::channel(8);
        let recorder = spawn_failure_recorder(storage::stores::Peerstore::new(store.clone()), rx);

        // A failure for the admitted generation refreshes the retry obligation
        // and rotates its token, retiring duplicate queued jobs from that run.
        let refreshed_generation =
            record_terminal_failure(&tx, admitted_generation, Some(Duration::from_secs(45)))
                .await
                .expect("current admitted failure should refresh the marker");
        assert!(refreshed_generation > admitted_generation);
        let retry_info = peerstore.get_retry_info("peer-a").await.unwrap().unwrap();
        let retry_info = storage::stores::RetryInfo::from_bytes(&retry_info).unwrap();
        assert!(retry_info.not_before_unix > 0);

        // A newer head supersedes the retry token. A late failure from the old
        // run must not replace that newer obligation.
        let newest_generation = peerstore
            .observe_push_head_with_generation("peer-a", "doc-a", "collection-a")
            .await
            .unwrap()
            .unwrap();
        assert!(newest_generation > refreshed_generation);
        assert_eq!(
            record_terminal_failure(&tx, refreshed_generation, None).await,
            None
        );
        let markers = peerstore.get_retry_documents("peer-a").await.unwrap();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].generation, newest_generation);

        // An already-admitted I/O can finish after forget, but its late error
        // cannot recreate the forgotten peer's marker or retry schedule.
        peerstore.delete_replicator("peer-a").await.unwrap();
        assert_eq!(
            record_terminal_failure(&tx, newest_generation, None).await,
            None
        );
        assert!(peerstore
            .get_retry_documents("peer-a")
            .await
            .unwrap()
            .is_empty());
        assert!(peerstore.get_retry_info("peer-a").await.unwrap().is_none());

        drop(tx);
        recorder.await.unwrap();
    }

    #[nuclei::test]
    async fn missing_worker_generation_does_not_fall_back_to_initial_registration() {
        let store = Arc::new(storage::backends::RegolithStore::in_memory().unwrap());
        let peerstore = storage::stores::Peerstore::new(store.clone());
        let replicator =
            p2p::ReplicatorInfo::from_raw("peer-a".into(), vec!["collection-a".into()], Vec::new());
        peerstore
            .create_replicator("peer-a", &replicator.to_bytes().unwrap())
            .await
            .unwrap();

        let (tx, rx) = p2p::rt::mpsc::channel(4);
        let recorder = spawn_failure_recorder(storage::stores::Peerstore::new(store.clone()), rx);

        // A terminal worker failure with a missing source token is dropped;
        // absence never silently switches into initial registration mode.
        assert_eq!(record_initial_failure(&tx, false).await, None);
        assert!(peerstore
            .get_retry_documents("peer-a")
            .await
            .unwrap()
            .is_empty());
        assert!(peerstore.get_retry_info("peer-a").await.unwrap().is_none());

        // The SE path has no admitted PushJob, so its explicit initial
        // registration signal is still allowed to create the obligation.
        let generation = record_initial_failure(&tx, true)
            .await
            .expect("explicit initial registration should create a marker");
        let markers = peerstore.get_retry_documents("peer-a").await.unwrap();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].generation, generation);

        drop(tx);
        recorder.await.unwrap();
    }
}
