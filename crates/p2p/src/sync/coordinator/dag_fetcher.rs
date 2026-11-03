//! Poll-based DAG fetcher for DocSync and BranchableSync.
//!
//! Tries a bounded CAR descendant closure from the known missing frontier,
//! then recomputes that frontier for any remaining blocks.
//! Stalled batches rotate across the context's providers under a per-attempt
//! stall budget, timed-out transport queries are cancelled at the end of their
//! poll window, and incomplete fetches are retried with backoff before the
//! failure is escalated (#1093).

use std::sync::Arc;
use std::time::Duration;

use blockstore::Blockstore;
use cid::Cid;
use n0_future::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::closure_depth::ClosureDepth;
use super::dag_context::DagFetchContext;
use super::dag_retry::{
    retry_backoff, ProviderRotation, BUSY_RETRIES_PER_DISPATCH, CLOCK_ATTEMPT_QUANTUM,
    FAST_REFUSAL, MAX_FETCH_ATTEMPTS, MAX_FREE_REFUSALS,
};
use super::fetch_failures::{FetchFailures, WindowFailure};
use super::fetch_logs::{log_exhausted, log_no_progress_batch, log_retry};
use super::provider_health::ProviderHealth;
use super::DagFetchLimiter;
use crate::sync::arbitration::AttemptSample;
use crate::sync::manager::links::find_all_missing_links;
use crate::sync::manager::SyncEvent;
use crate::sync::manager::{FetchCompletion, SyncDiagnostics};
use crate::transport::{FetchFailureCause, P2PTransport, PeerId};

const SELECTIVE_FETCH_BATCH_SIZE: usize = 2048;
/// Outer owner watchdog for a transport block-sync response.
///
/// Iroh bounds its request/response read at 30 seconds. The receiver owner must
/// outlive that bound so the transport completion event, rather than a racing
/// shorter poll window, decides when a productive CAR stream is reaped.
pub(super) const BLOCK_SYNC_COMPLETION_WATCHDOG: Duration = Duration::from_secs(35);

// The free refusals of one attempt together cost at most one fetch window.
const _: () = assert!(
    MAX_FREE_REFUSALS as u64 * FAST_REFUSAL.as_secs() <= BLOCK_SYNC_COMPLETION_WATCHDOG.as_secs()
);

/// How long a successful transport completion may wait for its blocks to
/// become durable locally. The libp2p host reports success once it has
/// forwarded every block event, and the coordinator stores those events on
/// independent tasks, so the completion regularly overtakes the puts (B0
/// rust-0: 4 ms lead, ~25 ms per put). Reading that as a stall cost the
/// dispatch a 2 s retry backoff while it held a fetch slot.
const SUCCESS_LANDING_GRACE: Duration = Duration::from_secs(1);
/// Defensive ceiling on selective-fetch DAG-walk iterations.
///
/// Each iteration reveals and fetches the next frontier of missing blocks
/// (`find_all_missing_links` can only traverse into blocks already present), so
/// the walk runs roughly once per layer of the missing sub-DAG. The real loop
/// terminator is the `!made_progress` break — this ceiling only guards against a
/// peer that dribbles blocks indefinitely. It must stay well above any realistic
/// DAG depth: the previous fixed cap of 20 stranded any document with a longer
/// unreplicated update history unmerged, even while the walk was still making
/// progress every iteration.
const MAX_DAG_WALK_ITERATIONS: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
enum FetchBatchOutcome {
    Complete,
    Partial,
    NoProgress(FetchFailures),
    Deferred,
    ProviderBusy,
    Unservable,
}

/// Outcome of one provider's poll window, as seen by the rotation loop.
///
/// `Stalled` (the watchdog elapsed with zero blocks), `Failed` (the transport
/// failed, but only after `FAST_REFUSAL`) and `Superseded` consume attempt
/// stall budget alike; `FailedFast` (the transport failed within
/// `FAST_REFUSAL`) spends a free refusal first; `SendFailed` (the query never
/// left) and `Busy` (the provider shed it) cost a rotation slot but no time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderWindowOutcome {
    Complete,
    Partial,
    Stalled,
    Failed(FetchFailureCause),
    Superseded,
    SendFailed,
    FailedFast(FetchFailureCause),
    Busy,
    Deferred,
    SizeLimit(Cid),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FetchAttemptOutcome {
    /// The DAG is local; carries the root block.
    Complete(bytes::Bytes),
    /// The missing block count, and why this attempt's windows failed.
    Incomplete(usize, FetchFailures),
    Deferred,
    ProviderBusy,
    Unservable,
    /// A clock-dispatched attempt still making progress reached its quantum.
    Preempted,
}

/// How a fetch task ended, for the per-root clock that dispatched it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FetchTaskEnd {
    /// Done for this dispatch: the root is due again a backoff from now.
    Settled,
    /// Still making progress at its quantum: due again at once, behind every
    /// root already due.
    Preempted,
}

/// Fetch an entire DAG rooted at `root_cid`.
///
/// Strategy: try CAR fetch first (one round-trip), then selective block fetch
/// for any missing blocks, rotating providers on stalls. Incomplete attempts
/// are retried with backoff; terminal failure is logged at ERROR because the
/// document cannot converge until the root is announced again.
///
/// A fetch the per-root retry clock dispatched makes one attempt and returns:
/// the clock owns that root's retries and backoff, and a slot held through
/// retries would keep every other due root waiting behind it. Such a root is
/// reported exhausted once per `MAX_FETCH_ATTEMPTS` incomplete attempts. An
/// attempt that keeps making progress ends at `CLOCK_ATTEMPT_QUANTUM`, at its
/// next window boundary, keeping what it fetched (`FetchTaskEnd::Preempted`).
/// The `AttemptSample` beside it feeds the root's estimators and nothing else.
///
/// A limiter permit is held only for the duration of each attempt — it is
/// released during backoff sleeps so a fetch stuck on dead providers does not
/// starve other roots under fan-in pressure, and before a complete DAG waits
/// for room in the merge loop's channel. A clock-dispatched fetch arrives
/// with its source peer's permit, taken together with its slot, and waits
/// only for a global one. Every issued transport query is reaped via
/// `cancel_sync` at the end of its poll window, so releasing the permit never
/// leaves stalled fetch work running in the transport.
pub async fn poll_fetch_dag<B: Blockstore + 'static, T: P2PTransport>(
    transport: T,
    blockstore: Arc<B>,
    event_tx: mpsc::Sender<SyncEvent>,
    root_cid: Cid,
    mut context: DagFetchContext,
    limiter: DagFetchLimiter,
    diagnostics: Arc<SyncDiagnostics>,
) -> (FetchTaskEnd, AttemptSample) {
    // libp2p keeps the context's order: its rooted CAR goes to the announcing
    // publisher, whose reply is what lets the dead-Bitswap repair fire.
    let health = transport
        .supports_cancellable_rooted_sync()
        .then(|| limiter.provider_health());
    let mut providers = match health {
        Some(health) => {
            let stats = transport.fetch_stats().await.unwrap_or_default();
            let ranking = health.rank(context.providers(), &stats, Instant::now());
            diagnostics.record_provider_ranking(ranking.demoted, ranking.cooling);
            ProviderRotation::new(ranking.peers)
        }
        None => ProviderRotation::new(context.providers()),
    };
    debug!(
        root_cid = %root_cid,
        doc_id = %context.doc_id,
        collection_id = %context.collection_id,
        source_peer = %context.source_peer,
        provider_count = providers.len(),
        is_explicit_replicator = context.is_explicit_replicator,
        "Starting DAG fetch (CAR-first, selective block fallback)"
    );

    // A root may already have completed through another same-CID arrival
    // between the clock claim and this task starting. Do not require a live
    // provider for local terminal progress.
    if let Ok(Some(root_data)) = blockstore.get(&root_cid).await {
        if find_all_missing_links(blockstore.as_ref(), &root_data)
            .await
            .is_ok_and(|missing| missing.is_empty())
        {
            emit_dag_ready(&event_tx, root_cid, &context, &root_data).await;
            return (FetchTaskEnd::Settled, AttemptSample::Complete);
        }
    }

    // A durable provider identity is reconnectable, not continuously
    // connected. When every qualified provider is temporarily unavailable,
    // leave the obligation with the authoritative per-root retry clock. This
    // is a paced deferral, not bounded-fetch exhaustion and not another retry
    // owner.
    if no_provider_is_connected(&transport, providers.peers(), &root_cid).await {
        diagnostics.record_pending_dag_fetch_deferred_unavailable();
        debug!(
            root_cid = %root_cid,
            providers = ?providers.peers(),
            "Deferring pending DAG fetch until a qualified provider reconnects"
        );
        return (FetchTaskEnd::Settled, AttemptSample::Skipped);
    }

    let mut remaining_count = 0usize;
    let mut last_failures = FetchFailures::default();
    let mut sample = AttemptSample::Skipped;
    let (attempts, mut busy_retries) = if context.is_clock_owned() {
        (1, 0)
    } else {
        (MAX_FETCH_ATTEMPTS, BUSY_RETRIES_PER_DISPATCH)
    };
    for attempt in 1..=attempts {
        if !context.is_current() {
            debug!(
                root_cid = %root_cid,
                doc_id = %context.doc_id,
                collection_id = %context.collection_id,
                "Stopping fetch for superseded sender/scope head"
            );
            return (FetchTaskEnd::Settled, sample);
        }
        if attempt > 1 {
            let backoff = retry_backoff(attempt);
            log_retry(
                &root_cid,
                &context,
                attempt,
                remaining_count,
                backoff,
                &last_failures,
                diagnostics.dag_fetch_logs(),
            );
            n0_future::time::sleep(backoff).await;
        }

        // A shed attempt repeats in place without consuming `attempt`.
        let outcome = loop {
            let reserved = context.take_peer_permit();
            let Some(permits) = limiter.acquire(&context.source_peer, reserved).await else {
                debug!(
                    root_cid = %root_cid,
                    doc_id = %context.doc_id,
                    attempt = attempt,
                    "DAG fetch limiter closed, abandoning fetch"
                );
                return (FetchTaskEnd::Settled, sample);
            };
            if !context.is_current() {
                return (FetchTaskEnd::Settled, sample);
            }
            let started = Instant::now();
            let mut state = ProviderFetchState::new(&mut providers, health, diagnostics.as_ref())
                .with_quantum(context.is_clock_owned());
            let outcome =
                fetch_dag_attempt(&transport, &blockstore, root_cid, &context, &mut state).await;
            sample = attempt_sample(&outcome, started.elapsed(), providers.attempt_progressed());
            if outcome != FetchAttemptOutcome::ProviderBusy || busy_retries == 0 {
                break outcome;
            }
            busy_retries -= 1;
            drop(permits);
            debug!(
                root_cid = %root_cid,
                attempt,
                "Busy; retrying once without spending an attempt"
            );
            n0_future::time::sleep(retry_backoff(attempt + 1)).await;
            if !context.is_current() {
                return (FetchTaskEnd::Settled, sample);
            }
        };

        match outcome {
            FetchAttemptOutcome::Complete(root_data) => {
                emit_dag_ready(&event_tx, root_cid, &context, &root_data).await;
                return (FetchTaskEnd::Settled, sample);
            }
            FetchAttemptOutcome::Unservable => {
                error!(root_cid = %root_cid, providers = ?providers.peers(),
                    "Providers cannot serve the remaining DAG within their CAR byte limits; stopping this fetch");
                return (FetchTaskEnd::Settled, sample);
            }
            FetchAttemptOutcome::Incomplete(remaining, failures) => {
                remaining_count = remaining;
                last_failures = failures;
            }
            FetchAttemptOutcome::Deferred => {
                diagnostics.record_pending_dag_fetch_deferred_contention();
                debug!(
                    root_cid = %root_cid,
                    "CAR ingest coalesced behind a storage owner; deferring to the per-root clock"
                );
                return (FetchTaskEnd::Settled, sample);
            }
            FetchAttemptOutcome::ProviderBusy => {
                diagnostics.record_pending_dag_fetch_deferred_busy();
                debug!(root_cid = %root_cid, providers = ?providers.peers(),
                    "A provider shed the fetch as busy and none delivered; deferring to the per-root clock");
                return (FetchTaskEnd::Settled, sample);
            }
            FetchAttemptOutcome::Preempted => {
                debug!(
                    root_cid = %root_cid,
                    quantum_secs = CLOCK_ATTEMPT_QUANTUM.as_secs(),
                    "DAG fetch still making progress at its quantum; giving its slot back"
                );
                return (FetchTaskEnd::Preempted, sample);
            }
        }
    }

    // A provider may disconnect after dispatch or during its bounded windows.
    // Preserve the distinction at terminal accounting: the durable root is
    // still actionable, and the existing clock will retry after reconnect.
    if no_provider_is_connected(&transport, providers.peers(), &root_cid).await {
        diagnostics.record_pending_dag_fetch_deferred_unavailable();
        debug!(
            root_cid = %root_cid,
            providers = ?providers.peers(),
            remaining_count,
            "Qualified providers disconnected during fetch; deferring to the per-root clock"
        );
        return (FetchTaskEnd::Settled, sample);
    }

    // A clock-dispatched fetch made one attempt. Its root has used up a fetch
    // budget, and is reported exhausted, once every MAX_FETCH_ATTEMPTS
    // incomplete attempts: the attempts a reply-driven fetch makes in place.
    let attempts = if context.is_clock_owned() {
        match context.record_incomplete_attempt() {
            Some(attempts) if attempts.is_multiple_of(MAX_FETCH_ATTEMPTS) => attempts,
            Some(attempts) => {
                debug!(
                    root_cid = %root_cid,
                    attempts,
                    remaining_count,
                    "DAG fetch incomplete; the per-root clock retries it after its backoff"
                );
                return (FetchTaskEnd::Settled, sample);
            }
            None => return (FetchTaskEnd::Settled, sample),
        }
    } else {
        MAX_FETCH_ATTEMPTS
    };

    diagnostics.record_pending_dag_fetch_exhausted();
    log_exhausted(
        &root_cid,
        &context,
        remaining_count,
        attempts,
        &providers,
        &last_failures,
        diagnostics.dag_fetch_logs(),
    );
    drop_dead_bitswap_connection(&transport, &providers, &context, &root_cid).await;
    (FetchTaskEnd::Settled, sample)
}

/// What an attempt that ended with `outcome` after `elapsed` tells its root's
/// estimators.
fn attempt_sample(
    outcome: &FetchAttemptOutcome,
    elapsed: Duration,
    progressed: bool,
) -> AttemptSample {
    match outcome {
        FetchAttemptOutcome::Complete(_) => AttemptSample::Complete,
        FetchAttemptOutcome::ProviderBusy => AttemptSample::Shed { progressed },
        FetchAttemptOutcome::Preempted => AttemptSample::Preempted,
        FetchAttemptOutcome::Incomplete(..) => AttemptSample::Incomplete {
            duration: elapsed,
            fast: elapsed < FAST_REFUSAL,
            progressed,
        },
        FetchAttemptOutcome::Deferred | FetchAttemptOutcome::Unservable => AttemptSample::Skipped,
    }
}

/// Hang up on a connection whose Bitswap is dead while the connection lives.
///
/// A partition that never closes the socket can leave a peer's Bitswap
/// queue dead on the outbound substream timeout, so every later want is
/// dropped before it is transmitted and the per-root clock would refetch over
/// the same dead connection forever. The bitswap crate keeps no long-lived
/// per-peer queue (each send stands alone), but Go peers do.
///
/// This is defence in depth for those Go peers, and for send failures on
/// transports that keep their own queues, so it is deliberately narrow. Every gate must hold:
///
/// - **libp2p only.** Iroh has no Bitswap message queue and retires its own
///   unresponsive connections (#1751).
/// - **The announcing publisher only.** It advertised this root, so it holds
///   the DAG; an alternate provider is any connected peer and serves nothing
///   simply because it has nothing to serve.
/// - **Positive transport-level liveness.** The peer answered a rooted CAR
///   request on its own substreams during this fetch. Bitswap silence alone
///   is not evidence: a want that never left and a want the peer cannot
///   satisfy are indistinguishable from here.
/// - **Still connected**, so the hang-up is the repair and not a race with a
///   connection that is already going away.
async fn drop_dead_bitswap_connection<T: P2PTransport>(
    transport: &T,
    providers: &ProviderRotation,
    context: &DagFetchContext,
    root_cid: &Cid,
) {
    if transport.supports_cancellable_rooted_sync() {
        return;
    }
    let provider = &context.source_peer;
    if !providers.is_responsive_but_silent(provider) {
        return;
    }
    let Ok(connected) = transport.connected_peers().await else {
        return;
    };
    if !connected.contains(provider) {
        return;
    }
    warn!(
        root_cid = %root_cid,
        provider = %provider,
        "Publisher answered on its own substreams but served no block for the root it announced; dropping the connection so Bitswap is rebuilt"
    );
    if let Err(error) = transport.disconnect(provider).await {
        debug!(root_cid = %root_cid, provider = %provider, error = %error, "Failed to drop the dead Bitswap connection");
    }
}

/// Return true only when the transport positively reports that none of the
/// root's qualified providers is connected. A peer-listing error is not proof
/// of unavailability, so the bounded fetch path remains available in that
/// case.
async fn no_provider_is_connected<T: P2PTransport>(
    transport: &T,
    providers: &[PeerId],
    root_cid: &Cid,
) -> bool {
    match transport.connected_peers().await {
        Ok(connected) => !providers
            .iter()
            .any(|provider| connected.contains(provider)),
        Err(error) => {
            debug!(
                root_cid = %root_cid,
                error = %error,
                "Could not determine provider connectivity; retaining bounded fetch behavior"
            );
            false
        }
    }
}

/// Connected peers to offer as alternate providers for a DAG fetch.
///
/// A transport lookup failure costs only rotation capability, not the fetch
/// itself, so it degrades to source-peer-only fetching — with a WARN trace,
/// because a silently empty alternates list is indistinguishable from a
/// healthy but sparsely connected node.
pub(crate) async fn connected_alternate_providers<T: P2PTransport>(
    transport: &T,
    root_cid: &Cid,
) -> Vec<PeerId> {
    match transport.connected_peers().await {
        Ok(peers) => peers,
        Err(e) => {
            warn!(
                root_cid = %root_cid,
                error = %e,
                "Failed to list connected peers for DAG fetch alternates; fetching from source peer only"
            );
            Vec::new()
        }
    }
}

/// Connected peers, then the peers whose service record says they hold
/// `collection_id`.
pub(crate) async fn alternate_providers<T: P2PTransport>(
    transport: &T,
    root_cid: &Cid,
    collection_id: &str,
    source_peer: &PeerId,
) -> Vec<PeerId> {
    let mut providers = connected_alternate_providers(transport, root_cid).await;
    add_collection_providers(transport, collection_id, source_peer, &mut providers).await;
    providers
}

/// Appends the peers whose service record says they hold `collection_id`,
/// after the ones already listed: the rotation tries them in order, so a
/// node we would have to dial is asked only once the others fail. The best
/// of them takes the last alternate slot the fetch context keeps, so a
/// well-connected node still reaches one.
pub(crate) async fn add_collection_providers<T: P2PTransport>(
    transport: &T,
    collection_id: &str,
    source_peer: &PeerId,
    providers: &mut Vec<PeerId>,
) {
    if collection_id.is_empty() {
        return;
    }
    let records = transport
        .collection_providers(collection_id)
        .await
        .unwrap_or_default();
    let slot = crate::sync::pending_store::MAX_PENDING_DAG_ALTERNATE_PROVIDERS - 1;
    if let Some(best) = records.iter().find(|peer| *peer != source_peer) {
        providers.retain(|peer| peer != source_peer);
        if providers
            .iter()
            .position(|peer| peer == best)
            .is_none_or(|index| index >= slot)
        {
            providers.retain(|peer| peer != best);
            providers.insert(slot.min(providers.len()), best.clone());
        }
    }
    for peer in records {
        if !providers.contains(&peer) {
            providers.push(peer);
        }
    }
}

/// One full CAR-first + selective-walk attempt over the current provider set.
async fn fetch_dag_attempt<B: Blockstore + 'static, T: P2PTransport>(
    transport: &T,
    blockstore: &Arc<B>,
    root_cid: Cid,
    context: &DagFetchContext,
    state: &mut ProviderFetchState<'_>,
) -> FetchAttemptOutcome {
    state.providers.begin_attempt();
    let car_missing_watch = match blockstore.get(&root_cid).await {
        Ok(Some(root_data)) => find_all_missing_links(blockstore.as_ref(), &root_data)
            .await
            .ok()
            .filter(|missing| !missing.is_empty()),
        _ => None,
    };

    // Sender demotion removes per-CID provider advertisements. Exercise the
    // rooted authority at the announcing publisher with a cancellable,
    // bounded descendant closure rooted at the already-known missing
    // frontier. This mirrors Go's one per-root blockservice session without
    // returning to an unbounded recursive historical-root walk.
    let rooted_wants = car_missing_watch
        .as_deref()
        .unwrap_or(std::slice::from_ref(&root_cid));
    let mut car_answered = false;
    // A missing root is usually a new head over held history, which a
    // whole-DAG CAR would re-send; on iroh the rotating fetch below depth-cuts it.
    let rooted_asked = !state.providers.cannot_serve(rooted_wants)
        && match car_missing_watch {
            Some(_) => context.needs_rooted_provider_discovery(),
            None => !transport.supports_cancellable_rooted_sync(),
        };
    let rooted_outcome = if !rooted_asked {
        ProviderWindowOutcome::SendFailed
    } else if car_missing_watch.is_some() && transport.supports_cancellable_rooted_sync() {
        poll_fetch_rooted_provider(
            &root_cid,
            rooted_wants,
            transport,
            blockstore,
            state.providers.current(),
            context,
            state.depth,
        )
        .await
    } else {
        poll_fetch_rooted_car(
            &root_cid,
            rooted_wants,
            transport,
            blockstore,
            state.providers.current(),
            context,
            &mut car_answered,
        )
        .await
    };
    if car_answered {
        state.providers.record_responsive();
    }
    if rooted_asked {
        record_health(
            state.health,
            context,
            state.providers.current(),
            rooted_outcome,
        );
    }
    if let ProviderWindowOutcome::SizeLimit(cid) = rooted_outcome {
        // libp2p can fall back to Bitswap, whose limits are independent of CAR.
        if transport.supports_cancellable_rooted_sync() {
            state.providers.record_size_limit(cid);
        }
    }
    if rooted_outcome == ProviderWindowOutcome::Deferred {
        return FetchAttemptOutcome::Deferred;
    }
    if rooted_outcome == ProviderWindowOutcome::Busy {
        state.providers.rotate_past_busy(state.diagnostics);
    }
    if matches!(
        rooted_outcome,
        ProviderWindowOutcome::Complete | ProviderWindowOutcome::Partial
    ) {
        state.providers.record_progress();
        state.depth.widen();
        if let Ok(Some(root_data)) = blockstore.get(&root_cid).await {
            let missing = find_all_missing_links(blockstore.as_ref(), &root_data)
                .await
                .unwrap_or_default();
            if missing.is_empty() {
                info!(root_cid = %root_cid, doc_id = %context.doc_id, "DAG fetch complete via rooted CAR");
                return FetchAttemptOutcome::Complete(root_data);
            }
            debug!(
                root_cid = %root_cid,
                missing_count = missing.len(),
                "CAR fetch was partial, falling through to selective block fetch"
            );
        }
    }

    // Fallback fetch: fetch the root block first so we can enumerate missing links.
    let root_was_missing = !matches!(blockstore.has(&root_cid).await, Ok(true));
    match poll_fetch_blocks_rotating(
        &root_cid,
        std::slice::from_ref(&root_cid),
        transport,
        blockstore,
        state,
        context,
    )
    .await
    {
        FetchBatchOutcome::Complete => {
            if root_was_missing {
                state.depth.widen();
            }
        }
        FetchBatchOutcome::Deferred => return FetchAttemptOutcome::Deferred,
        FetchBatchOutcome::ProviderBusy => return FetchAttemptOutcome::ProviderBusy,
        FetchBatchOutcome::Unservable => return FetchAttemptOutcome::Unservable,
        FetchBatchOutcome::Partial => {
            warn!(root_cid = %root_cid, "Failed to fetch root block");
            return incomplete_or_shed(state.providers, 1, FetchFailures::default());
        }
        FetchBatchOutcome::NoProgress(failures) => {
            log_no_progress_batch(&root_cid, 1, &failures, state.diagnostics.dag_fetch_logs());
            return incomplete_or_shed(state.providers, 1, failures);
        }
    }

    // Walk DAG, fetching missing blocks level by level. The walk stops as soon
    // as an iteration makes no progress (`!made_progress` below), and a
    // clock-dispatched walk that is still progressing stops at its quantum,
    // keeping what it fetched; the iteration ceiling is only a defensive
    // backstop, not a functional depth limit.
    let mut unservable = false;
    let mut attempt_failures = FetchFailures::default();
    for iteration in 0..MAX_DAG_WALK_ITERATIONS {
        if iteration > 0 && state.quantum_spent() {
            return FetchAttemptOutcome::Preempted;
        }
        let root_data = match blockstore.get(&root_cid).await {
            Ok(Some(data)) => data,
            _ => {
                warn!(root_cid = %root_cid, "Root block disappeared from blockstore");
                return FetchAttemptOutcome::Incomplete(1, attempt_failures);
            }
        };

        let missing = match find_all_missing_links(blockstore.as_ref(), &root_data).await {
            Ok(m) => m,
            Err(e) => {
                warn!(root_cid = %root_cid, error = %e, "find_all_missing_links failed");
                return FetchAttemptOutcome::Incomplete(0, attempt_failures);
            }
        };

        if missing.is_empty() {
            break;
        }

        debug!(
            root_cid = %root_cid,
            iteration = iteration,
            missing_count = missing.len(),
            "Fetching missing DAG blocks via selective block fetch"
        );

        let mut made_progress = false;
        for batch in missing.chunks(SELECTIVE_FETCH_BATCH_SIZE) {
            if made_progress && state.quantum_spent() {
                return FetchAttemptOutcome::Preempted;
            }
            match poll_fetch_blocks_rotating(
                &root_cid, batch, transport, blockstore, state, context,
            )
            .await
            {
                FetchBatchOutcome::Complete => {
                    made_progress = true;
                }
                FetchBatchOutcome::Partial => {
                    made_progress = true;
                    debug!(
                        root_cid = %root_cid,
                        requested_count = batch.len(),
                        "Selective block batch made partial progress; continuing DAG walk"
                    );
                }
                FetchBatchOutcome::NoProgress(failures) => {
                    log_no_progress_batch(
                        &root_cid,
                        batch.len(),
                        &failures,
                        state.diagnostics.dag_fetch_logs(),
                    );
                    attempt_failures.merge(&failures);
                }
                FetchBatchOutcome::Deferred => return FetchAttemptOutcome::Deferred,
                FetchBatchOutcome::ProviderBusy => return FetchAttemptOutcome::ProviderBusy,
                FetchBatchOutcome::Unservable => unservable = true,
            }
        }
        if !made_progress {
            break;
        }
        state.depth.widen();
    }

    // Verify DAG is complete
    let root_data = match blockstore.get(&root_cid).await {
        Ok(Some(data)) => data,
        _ => return FetchAttemptOutcome::Incomplete(1, attempt_failures),
    };
    let remaining = find_all_missing_links(blockstore.as_ref(), &root_data)
        .await
        .unwrap_or_default();

    if remaining.is_empty() {
        info!(root_cid = %root_cid, doc_id = %context.doc_id, "DAG fetch complete");
        FetchAttemptOutcome::Complete(root_data)
    } else if unservable {
        FetchAttemptOutcome::Unservable
    } else {
        incomplete_or_shed(state.providers, remaining.len(), attempt_failures)
    }
}

/// A shed attempt with no delivery proves saturation, not an unfetchable DAG:
/// a busy hub's alternates often do not hold a fresh write yet and answer empty.
/// A batch that every provider shed releases as busy directly, without this.
fn incomplete_or_shed(
    providers: &ProviderRotation,
    remaining: usize,
    failures: FetchFailures,
) -> FetchAttemptOutcome {
    if providers.attempt_was_shed() {
        FetchAttemptOutcome::ProviderBusy
    } else {
        FetchAttemptOutcome::Incomplete(remaining, failures)
    }
}

async fn emit_dag_ready(
    event_tx: &mpsc::Sender<SyncEvent>,
    root_cid: Cid,
    context: &DagFetchContext,
    root_data: &[u8],
) {
    let mut context = context.clone();
    context.fill_missing_from_block(root_data);
    if event_tx
        .send(context.into_dag_ready(root_cid))
        .await
        .is_err()
    {
        warn!(
            root_cid = %root_cid,
            "Failed to emit DagReady after DAG fetch"
        );
    }
}

/// Exercise the established libp2p CAR request protocol when the transport's
/// block-sync primitive cannot express a recursive root request.  Sending the
/// request and receiving its CAR response are separate streams on libp2p, so
/// completion is established from the bounded rooted frontier in blockstore.
///
/// `answered` is set when the peer's CAR response reaches this node, with or
/// without blocks in it. That round trip is the only transport-level liveness
/// evidence this fetch collects.
async fn poll_fetch_rooted_car<B: Blockstore, T: P2PTransport>(
    root_cid: &Cid,
    watch_cids: &[Cid],
    transport: &T,
    blockstore: &Arc<B>,
    source_peer: &PeerId,
    context: &DagFetchContext,
    answered: &mut bool,
) -> ProviderWindowOutcome {
    let mut initially_missing = 0usize;
    for cid in watch_cids {
        if !matches!(blockstore.has(cid).await, Ok(true)) {
            initially_missing += 1;
        }
    }
    if initially_missing == 0 {
        return ProviderWindowOutcome::Complete;
    }

    let mut completion = context.track_rooted_car(*root_cid, source_peer);

    match n0_future::time::timeout(
        Duration::from_secs(10),
        transport.send_car_request(source_peer, *root_cid),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            context.cancel_rooted_car_tracking(*root_cid);
            debug!(root_cid = %root_cid, source_peer = %source_peer, error = %error, "Rooted CAR request failed before dispatch");
            return ProviderWindowOutcome::SendFailed;
        }
        Err(_) => {
            context.cancel_rooted_car_tracking(*root_cid);
            return ProviderWindowOutcome::Stalled;
        }
    }

    let observe = |remaining: usize| {
        if remaining == 0 {
            ProviderWindowOutcome::Complete
        } else if remaining < initially_missing {
            ProviderWindowOutcome::Partial
        } else {
            ProviderWindowOutcome::Stalled
        }
    };
    let remaining_now = count_missing(blockstore, watch_cids).await;
    if remaining_now < initially_missing && completion.is_none() {
        context.cancel_rooted_car_tracking(*root_cid);
        return observe(remaining_now);
    }

    if let Some(receiver) = completion.as_mut() {
        let deadline = n0_future::time::Instant::now() + Duration::from_secs(10);
        loop {
            if !context.is_current() {
                context.cancel_rooted_car_tracking(*root_cid);
                return ProviderWindowOutcome::Superseded;
            }
            tokio::select! {
                result = &mut *receiver => {
                    // Busy is alive, not the silent publisher of a dead Bitswap link.
                    let busy = matches!(result, Ok(FetchCompletion::ProviderBusy));
                    *answered = result.is_ok() && !busy;
                    if matches!(result, Ok(FetchCompletion::Deferred)) {
                        context.cancel_rooted_car_tracking(*root_cid);
                        return ProviderWindowOutcome::Deferred;
                    }
                    if busy {
                        context.cancel_rooted_car_tracking(*root_cid);
                        return match observe(count_missing(blockstore, watch_cids).await) {
                            ProviderWindowOutcome::Stalled => ProviderWindowOutcome::Busy,
                            progressed => progressed,
                        };
                    }
                    if let Ok(FetchCompletion::SizeLimit(cid)) = result {
                        return ProviderWindowOutcome::SizeLimit(cid);
                    }
                    break;
                },
                _ = n0_future::time::sleep_until(deadline) => break,
                _ = n0_future::time::sleep(Duration::from_millis(100)) => {}
            }
            if n0_future::time::Instant::now() >= deadline {
                break;
            }
        }
    } else {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) && context.is_current() {
            let remaining = count_missing(blockstore, watch_cids).await;
            if remaining < initially_missing {
                return observe(remaining);
            }
            n0_future::time::sleep(Duration::from_millis(100)).await;
        }
    }
    context.cancel_rooted_car_tracking(*root_cid);
    observe(count_missing(blockstore, watch_cids).await)
}

async fn count_missing<B: Blockstore>(blockstore: &Arc<B>, cids: &[Cid]) -> usize {
    let mut remaining = 0usize;
    for cid in cids {
        if !matches!(blockstore.has(cid).await, Ok(true)) {
            remaining += 1;
        }
    }
    remaining
}

/// Exercise one authenticated provider's rooted CAR authority with the
/// missing frontier already known from the local root. Iroh returns a capped
/// descendant closure from that frontier; other transports retain their
/// native session/exact-want behavior. The query is cancellable and bounded,
/// so an unavailable provider cannot retain a transport task or monopolize the
/// receiver fetch owner.
async fn poll_fetch_rooted_provider<B: Blockstore, T: P2PTransport>(
    root_cid: &Cid,
    watch_cids: &[Cid],
    transport: &T,
    blockstore: &Arc<B>,
    source_peer: &PeerId,
    context: &DagFetchContext,
    depth: ClosureDepth,
) -> ProviderWindowOutcome {
    let mut initially_missing = 0usize;
    for cid in watch_cids {
        if !matches!(blockstore.has(cid).await, Ok(true)) {
            initially_missing += 1;
        }
    }
    if initially_missing == 0 {
        return ProviderWindowOutcome::Complete;
    }
    let query_id = match transport
        .sync_blocks_within(
            *root_cid,
            vec![source_peer.clone()],
            watch_cids.to_vec(),
            depth.get(),
        )
        .await
    {
        Ok(query_id) => query_id,
        Err(error) => {
            debug!(root_cid = %root_cid, source_peer = %source_peer, error = %error, "Rooted CAR request failed before dispatch");
            return ProviderWindowOutcome::SendFailed;
        }
    };
    let mut completion = context.track_block_sync(query_id);
    let completion_is_observable = completion.is_some();
    let start = Instant::now();
    let timeout = BLOCK_SYNC_COMPLETION_WATCHDOG;
    let mut outcome = ProviderWindowOutcome::Stalled;
    let mut transport_complete = false;
    let mut ended_early = None;
    let mut deferred = false;
    let mut size_limit = None;
    while start.elapsed() < timeout && context.is_current() {
        let mut remaining = 0usize;
        for cid in watch_cids {
            if !matches!(blockstore.has(cid).await, Ok(true)) {
                remaining += 1;
            }
        }
        if remaining == 0 {
            outcome = ProviderWindowOutcome::Complete;
            // Every CID requested by this selective CAR is durable locally.
            // There is no remaining response payload needed by this owner;
            // reaping the query here also makes an early transport-completion
            // notification harmless. Partial responses still drain below.
            break;
        }
        if remaining < initially_missing {
            outcome = ProviderWindowOutcome::Partial;
            // A CAR response is a stream.  Seeing its first useful block is
            // progress, not completion: cancelling here truncates the rooted
            // response and leaves the newly discovered frontier unavailable
            // from the only authoritative publisher.  Production contexts
            // carry the transport completion tracker, so let that response
            // drain.  Direct unit transports without completion events retain
            // the blockstore-only fallback.
            if !completion_is_observable {
                break;
            }
        }
        if transport_complete {
            break;
        }
        if let Some(receiver) = completion.as_mut() {
            tokio::select! {
                result = receiver => {
                    let fetch_completion =
                        result.unwrap_or(FetchCompletion::Failure(FetchFailureCause::Error));
                    completion = None;
                    transport_complete = true;
                    match fetch_completion {
                        FetchCompletion::Success => {}
                        FetchCompletion::Failure(cause) => {
                            ended_early = Some(failure_outcome(cause, start.elapsed()));
                            break;
                        }
                        FetchCompletion::ProviderBusy => {
                            ended_early = Some(ProviderWindowOutcome::Busy);
                            break;
                        }
                        FetchCompletion::Deferred => {
                            deferred = true;
                            break;
                        }
                        FetchCompletion::SizeLimit(cid) => {
                            size_limit = Some(cid);
                            break;
                        }
                    }
                }
                _ = n0_future::time::sleep(Duration::from_millis(100)) => {}
            }
        } else {
            n0_future::time::sleep(Duration::from_millis(100)).await;
        }
    }
    context.cancel_block_sync_tracking(query_id);
    if let Err(error) = transport.cancel_sync(query_id).await {
        debug!(root_cid = %root_cid, query_id = query_id.0, error = %error, "Failed to cancel rooted CAR query");
    }
    window_outcome(outcome, ended_early, deferred, size_limit)
}

/// Fetch one batch of exact blocks, rotating to the next provider whenever
/// the current one yields nothing within the fetch window.
///
/// `stall_budget` caps the stalled (timed-out) provider windows one attempt
/// may burn across all of its batches; once it reaches zero, stalled batches
/// fail fast without issuing further transport queries.
struct ProviderFetchState<'a> {
    providers: &'a mut ProviderRotation,
    stall_budget: usize,
    free_refusals: usize,
    health: Option<&'a ProviderHealth>,
    diagnostics: &'a SyncDiagnostics,
    depth: ClosureDepth,
    /// When a clock-dispatched attempt that keeps making progress gives its
    /// slot back (`CLOCK_ATTEMPT_QUANTUM` from its start).
    quantum_ends: Option<n0_future::time::Instant>,
}

impl<'a> ProviderFetchState<'a> {
    /// One full rotation's worth of empty windows per attempt. Productive
    /// windows (any block arrived) never consume it, so a large DAG that is
    /// actually transferring is unbounded, while a dead provider set costs at
    /// most providers × BLOCK_SYNC_COMPLETION_WATCHDOG of stalled waiting per
    /// attempt regardless of how many batches the missing frontier spans.
    fn new(
        providers: &'a mut ProviderRotation,
        health: Option<&'a ProviderHealth>,
        diagnostics: &'a SyncDiagnostics,
    ) -> Self {
        Self {
            stall_budget: providers.len(),
            free_refusals: MAX_FREE_REFUSALS,
            depth: ClosureDepth::initial(),
            providers,
            health,
            diagnostics,
            quantum_ends: None,
        }
    }

    /// Bound a clock-dispatched attempt by `CLOCK_ATTEMPT_QUANTUM`.
    fn with_quantum(mut self, clock_owned: bool) -> Self {
        self.quantum_ends =
            clock_owned.then(|| n0_future::time::Instant::now() + CLOCK_ATTEMPT_QUANTUM);
        self
    }

    fn quantum_spent(&self) -> bool {
        self.quantum_ends
            .is_some_and(|end| n0_future::time::Instant::now() >= end)
    }
}

/// Only the announcing peer claimed to hold the root, so only its fast failure
/// reads as load; an alternate may not hold a just-published root yet. A busy
/// reply is explicit load from any provider, so it cools whoever sent it.
fn record_health(
    health: Option<&ProviderHealth>,
    context: &DagFetchContext,
    provider: &PeerId,
    outcome: ProviderWindowOutcome,
) {
    let Some(health) = health else { return };
    match outcome {
        ProviderWindowOutcome::Complete | ProviderWindowOutcome::Partial => {
            health.record_productive(provider);
        }
        ProviderWindowOutcome::FailedFast(_) | ProviderWindowOutcome::SendFailed
            if *provider == context.source_peer =>
        {
            health.record_fast_failure(provider, Instant::now());
        }
        ProviderWindowOutcome::Busy => health.record_fast_failure(provider, Instant::now()),
        _ => {}
    }
}

async fn poll_fetch_blocks_rotating<B: Blockstore, T: P2PTransport>(
    root_cid: &Cid,
    cids: &[Cid],
    transport: &T,
    blockstore: &Arc<B>,
    state: &mut ProviderFetchState<'_>,
    context: &DagFetchContext,
) -> FetchBatchOutcome {
    let mut missing = Vec::new();
    for cid in cids {
        if !matches!(blockstore.has(cid).await, Ok(true)) {
            missing.push(*cid);
        }
    }
    if missing.is_empty() {
        return FetchBatchOutcome::Complete;
    }
    let mut unservable = 0;
    let mut failures = FetchFailures::default();
    let mut busy = 0;
    for _ in 0..state.providers.len() {
        if state.providers.cannot_serve(&missing) {
            unservable += 1;
            failures.record(state.providers.current(), WindowFailure::Unservable);
            state.providers.advance();
            continue;
        }
        if state.providers.current_is_busy() {
            busy += 1;
            failures.record(state.providers.current(), WindowFailure::Busy);
            state.providers.advance();
            continue;
        }
        if state.stall_budget == 0 {
            debug!(
                root_cid = %root_cid,
                requested_count = cids.len(),
                "Attempt stall budget exhausted, failing batch without fetching"
            );
            failures.budget_exhausted += 1;
            return FetchBatchOutcome::NoProgress(failures);
        }
        let provider = state.providers.current().clone();
        let outcome = poll_fetch_blocks(
            root_cid,
            cids,
            transport,
            blockstore,
            &provider,
            context,
            state.depth,
        )
        .await;
        record_health(state.health, context, &provider, outcome);
        match outcome {
            ProviderWindowOutcome::Complete => {
                state.providers.record_progress();
                return FetchBatchOutcome::Complete;
            }
            ProviderWindowOutcome::Partial => {
                state.providers.record_progress();
                return FetchBatchOutcome::Partial;
            }
            window @ (ProviderWindowOutcome::Stalled
            | ProviderWindowOutcome::Failed(_)
            | ProviderWindowOutcome::FailedFast(_)
            | ProviderWindowOutcome::Superseded) => {
                if matches!(window, ProviderWindowOutcome::FailedFast(_)) && state.free_refusals > 0
                {
                    state.free_refusals -= 1;
                } else {
                    state.stall_budget -= 1;
                }
                state.providers.advance();
                state.diagnostics.record_provider_rotation();
                let failure = match window {
                    ProviderWindowOutcome::Failed(cause)
                    | ProviderWindowOutcome::FailedFast(cause) => WindowFailure::from(cause),
                    ProviderWindowOutcome::Superseded => WindowFailure::Superseded,
                    _ => WindowFailure::TimedOut,
                };
                failures.record(&provider, failure);
                debug!(
                    root_cid = %root_cid,
                    provider = %provider,
                    outcome = ?failure,
                    requested_count = cids.len(),
                    "No blocks from provider within its fetch window, rotating to next provider"
                );
            }
            ProviderWindowOutcome::SendFailed => {
                state.providers.advance();
                state.diagnostics.record_provider_rotation();
                failures.record(&provider, WindowFailure::SendFailed);
            }
            ProviderWindowOutcome::Busy => {
                state.providers.rotate_past_busy(state.diagnostics);
                busy += 1;
                failures.record(&provider, WindowFailure::Busy);
                debug!(
                    root_cid = %root_cid,
                    provider = %provider,
                    requested_count = cids.len(),
                    "Provider shed the fetch as busy, rotating to next provider"
                );
            }
            ProviderWindowOutcome::Deferred => return FetchBatchOutcome::Deferred,
            ProviderWindowOutcome::SizeLimit(cid) => {
                state.providers.record_size_limit(cid);
                state.providers.advance();
                state.diagnostics.record_provider_rotation();
                unservable += 1;
                failures.record(&provider, WindowFailure::Unservable);
            }
        }
    }
    let remaining = count_missing(blockstore, &missing).await;
    if remaining == 0 {
        FetchBatchOutcome::Complete
    } else if remaining < missing.len() {
        FetchBatchOutcome::Partial
    } else if unservable == state.providers.len() {
        FetchBatchOutcome::Unservable
    } else if busy > 0 && busy + unservable == state.providers.len() {
        FetchBatchOutcome::ProviderBusy
    } else {
        FetchBatchOutcome::NoProgress(failures)
    }
}

/// Fetch one batch of exact blocks via the transport's block-sync path.
///
/// The issued transport query is always reaped via `cancel_sync` before this
/// returns, so no query outlives its poll window: a stalled provider's
/// transport work cannot pile up behind rotation, keep consuming connections
/// after the fetch's limiter permit is released, or deliver blocks after the
/// fetch has terminally failed.
async fn poll_fetch_blocks<B: Blockstore, T: P2PTransport>(
    root_cid: &Cid,
    cids: &[Cid],
    transport: &T,
    blockstore: &Arc<B>,
    source_peer: &PeerId,
    context: &DagFetchContext,
    depth: ClosureDepth,
) -> ProviderWindowOutcome {
    let mut missing = Vec::new();
    for cid in cids {
        if matches!(blockstore.has(cid).await, Ok(true)) {
            continue;
        }
        missing.push(*cid);
    }

    if missing.is_empty() {
        return ProviderWindowOutcome::Complete;
    }

    let query_id = match transport
        .sync_blocks_within(
            *root_cid,
            vec![source_peer.clone()],
            missing.clone(),
            depth.get(),
        )
        .await
    {
        Ok(query_id) => query_id,
        Err(e) => {
            warn!(
                root_cid = %root_cid,
                requested_count = missing.len(),
                error = %e,
                "selective block fetch failed"
            );
            return ProviderWindowOutcome::SendFailed;
        }
    };
    let mut completion = context.track_block_sync(query_id);
    let completion_is_observable = completion.is_some();
    let mut landing_deadline: Option<Instant> = None;
    let mut deferred = false;
    let mut size_limit = None;

    let timeout = BLOCK_SYNC_COMPLETION_WATCHDOG;
    let start = Instant::now();
    let mut outcome = ProviderWindowOutcome::Stalled;
    let mut ended_early = None;
    // Once Success has been seen the landing deadline, not the watchdog,
    // bounds the loop: a completion that arrives in the watchdog's last
    // moments must still get the whole grace for its blocks to land.
    while landing_deadline.is_some() || start.elapsed() < timeout {
        if !context.is_current() {
            ended_early = Some(ProviderWindowOutcome::Superseded);
            break;
        }
        let mut remaining = 0usize;
        for cid in &missing {
            if !matches!(blockstore.has(cid).await, Ok(true)) {
                remaining += 1;
            }
        }
        if remaining == 0 {
            outcome = ProviderWindowOutcome::Complete;
            // Exact-CID recovery is complete once every requested block is
            // durable. Partial responses continue waiting for transport
            // completion so cancellation cannot truncate useful payload.
            break;
        }
        if remaining < missing.len() {
            outcome = ProviderWindowOutcome::Partial;
            // Do not abort a productive selective CAR stream after its first
            // block.  Its remaining requested blocks are still in flight.
            if !completion_is_observable {
                break;
            }
        }
        if landing_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            break;
        }
        if let Some(receiver) = completion.as_mut() {
            tokio::select! {
                result = receiver => {
                    let fetch_completion =
                        result.unwrap_or(FetchCompletion::Failure(FetchFailureCause::Error));
                    completion = None;
                    match fetch_completion {
                        FetchCompletion::Success => {
                            landing_deadline =
                                Some(Instant::now() + SUCCESS_LANDING_GRACE);
                        }
                        FetchCompletion::Failure(cause) => {
                            ended_early = Some(failure_outcome(cause, start.elapsed()));
                            break;
                        }
                        FetchCompletion::ProviderBusy => {
                            ended_early = Some(ProviderWindowOutcome::Busy);
                            break;
                        }
                        FetchCompletion::Deferred => {
                            deferred = true;
                            break;
                        }
                        FetchCompletion::SizeLimit(cid) => {
                            size_limit = Some(cid);
                            break;
                        }
                    }
                }
                _ = n0_future::time::sleep(Duration::from_millis(100)) => {}
            }
        } else {
            n0_future::time::sleep(Duration::from_millis(100)).await;
        }
    }

    context.cancel_block_sync_tracking(query_id);
    if let Err(e) = transport.cancel_sync(query_id).await {
        debug!(
            root_cid = %root_cid,
            query_id = query_id.0,
            error = %e,
            "Failed to cancel block-sync query after poll window"
        );
    }
    window_outcome(outcome, ended_early, deferred, size_limit)
}

/// A failure after `FAST_REFUSAL` (a response or connect timeout) stays
/// `Failed` and spends stall budget: free refusals are capped on the assumption
/// that each costs at most `FAST_REFUSAL`, so a slower one must spend budget to
/// keep the attempt bounded.
fn failure_outcome(cause: FetchFailureCause, elapsed: Duration) -> ProviderWindowOutcome {
    if elapsed < FAST_REFUSAL {
        ProviderWindowOutcome::FailedFast(cause)
    } else {
        ProviderWindowOutcome::Failed(cause)
    }
}

/// A window that ended before its watchdog (`ended_early`) reports why, unless
/// a size limit or deferral already explains it or a block arrived.
fn window_outcome(
    observed: ProviderWindowOutcome,
    ended_early: Option<ProviderWindowOutcome>,
    deferred: bool,
    size_limit: Option<Cid>,
) -> ProviderWindowOutcome {
    if let Some(cid) = size_limit {
        ProviderWindowOutcome::SizeLimit(cid)
    } else if deferred {
        ProviderWindowOutcome::Deferred
    } else if observed == ProviderWindowOutcome::Stalled {
        ended_early.unwrap_or(observed)
    } else {
        observed
    }
}

#[cfg(test)]
#[path = "dag_fetcher_tests.rs"]
mod tests;
