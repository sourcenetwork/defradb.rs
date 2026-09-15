//! One durable sender retry state machine shared by every runtime and transport.

use std::sync::Arc;
use std::time::Duration;

use p2p::transport::{P2PTransport, PeerAddr, PeerId};

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
                let info_bytes = match storage::stores::RetryInfo::new_initial().to_bytes() {
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
    if let Err(error) = transport.dial(peer_id, addrs).await {
        tracing::debug!(%peer_id, %error, "replicator retry redial failed");
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
        if !force && !markers.iter().any(|marker| marker.retry_info.is_due()) {
            continue;
        }

        let connected = transport.connected_peers().await.unwrap_or_default();
        if !connected.contains(&peer_id) {
            // Connectivity is part of the due retry attempt. Do not create a
            // second two-second redial clock for markers whose ladder has not
            // elapsed yet.
            redial_replicator(peerstore, transport, &peer_id).await;
            let _ = peerstore.reschedule_retry_peer(&peer_id_str, None, 1).await;
            finish_peer(peerstore, &peer_id_str, false).await;
            continue;
        }

        let deadline = n0_future::time::Instant::now() + MAX_PEER_PASS;
        let mut attempted = 0usize;
        let mut failed = false;
        let mut progressed = false;
        let mut deferred = false;
        let mut report = PassReport::default();
        for marker in &markers {
            if !force && !marker.retry_info.is_due() {
                continue;
            }
            if attempted >= MAX_MARKERS_PER_PEER_PASS || n0_future::time::Instant::now() >= deadline
            {
                report.truncated = true;
                break;
            }
            attempted += 1;
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
    })
}

#[cfg(test)]
#[path = "retry_sweep_tests.rs"]
mod sweep_tests;

#[cfg(test)]
mod tests {
    use super::*;

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
}
