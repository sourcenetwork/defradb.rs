//! Sync coordinator for DefraDB P2P synchronization.
//!
//! The coordinator ties together:
//! - P2P transport for network communication
//! - SyncManager for block storage and merge tracking
//! - Broadcaster for publishing updates
//!
//! # Architecture
//!
//! ```text
//! Database Layer
//!       ↓
//! SyncCoordinator<B, T>
//!       ├── Broadcaster<T> (publish updates)
//!       ├── SyncManager (store blocks, emit events)
//!       └── Event loop (receive TransportEvents)
//!       ↓
//! T: P2PTransport (network)
//! ```
//!
//! # Security Model: Go-Compatible Ingress + Merge-Time ACP
//!
//! Rust follows the Go DefraDB mental model:
//!
//! - **Replicator registration** expresses outbound replay intent and explicit
//!   replay trust. It controls what we push and which peers are treated as
//!   explicit replicators.
//! - **Direct replicator PushLog acceptance** follows Go's replicator comm
//!   channel and skips receiver-side collection access before merge.
//! - **Pubsub PushLog acceptance** requires collection replicator membership,
//!   a local collection subscription, or explicit replay authorization.
//! - **Inbound Gossip acceptance** treats a local collection subscription as
//!   receive intent, regardless of outbound replicator configuration. Without
//!   a subscription, outbound targets remain invalid gossip sources.
//! - **Document-level ACP** remains the authoritative policy boundary for whether
//!   replicated document content is actually mergeable/readable locally.
//!
//! Pull-sync protocols that mirror Go's `doc-sync` / `sync-branchable` RPCs
//! may be served to connected peers. Document-level ACP remains the
//! authoritative policy boundary for whether replicated document content is
//! mergeable/readable locally.

mod access;
mod accessors;
mod authorizer;
mod broadcast;
mod constructor;
pub(crate) mod dag_context;
pub(crate) mod dag_fetcher;
pub(crate) mod dag_retry;
mod event_handler;
#[cfg(feature = "libp2p-transport")]
mod pubsub_client;
#[cfg(feature = "libp2p-transport")]
mod pubsub_services;
mod push_worker;
mod replicators;
mod result_types;
mod selective_car_access;
mod subscriptions;

pub use result_types::{CreateReplicatorResult, LoadReplicatorsResult};
pub use selective_car_access::{HeadHintCarAuthority, HeadHintCarGrant};

use rapidhash::{HashMapExt, RapidHashMap};
use std::sync::atomic::{fence, AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use acp::DocumentACP;
use blockstore::Blockstore;
use cid::Cid;
use defra_core::thread_bounds::MaybeSend;
use kovan::{Atom, AtomOption};
use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use tokio::sync::{watch, Notify, OwnedSemaphorePermit, Semaphore};

use crate::bitswap::{AccessMode, ReplicatorRegistry};
use crate::replicator::ReplicationFilterMatcher;
use crate::tracked_task::{TrackedAbort, TrackedTask};
use crate::transport::{P2PTransport, PeerId};

use super::broadcaster::Broadcaster;
use super::collection_store::P2PCollectionStorage;
use super::head_provider::DocumentHeadProvider;
use super::manager::{PendingDag, SyncManager};
use super::peer_state::PeerStateTracker;
use super::rate_limiter::PeerRateLimiter;

#[cfg(test)]
pub(crate) use super::manager::{
    DEFAULT_MAX_CONCURRENT_DAG_FETCHES, DEFAULT_MAX_CONCURRENT_PUSH_TASKS,
    DEFAULT_MAX_DOC_SYNC_REQUEST_DOC_IDS,
};

/// A durable retry update emitted by outbound PushLog admission/workers.
///
/// The runtime consumes observations to register scope markers before send,
/// failures to retain/reschedule them, and current acknowledgements to clear them.
#[derive(Debug)]
pub struct PushFailure {
    pub peer_id: String,
    pub doc_id: String,
    pub collection_id: String,
    pub cid: String,
    pub head_priority: u64,
    /// False for a pre-send observation; true for a terminal failure that keeps
    /// the already-registered scope marker on its durable ladder.
    pub create_retry: bool,
    /// Successful acknowledgement for this exact scope/head.
    pub acknowledged: bool,
    /// Register-before-send handshake with the durable recorder.
    pub durable_tx: Option<tokio::sync::oneshot::Sender<bool>>,
}

/// Volatile fence for acknowledgements of presence-only durable markers.
///
/// The durable marker intentionally stores no CID. This fence covers the only
/// acknowledgements that can survive long enough to race a newer live head in
/// the same process. Across restart there are no surviving in-flight sends, so
/// the presence marker alone is the conservative source of truth.
#[derive(Default)]
pub struct HeadAckFence {
    current: rapidhash::RapidHashMap<(String, String, String), (u64, String)>,
}

impl HeadAckFence {
    fn scope_key(event: &PushFailure) -> (String, String, String) {
        (
            event.peer_id.clone(),
            event.doc_id.clone(),
            event.collection_id.clone(),
        )
    }

    fn head_token(event: &PushFailure) -> (u64, String) {
        (event.head_priority, event.cid.clone())
    }

    pub fn observe_durable(&mut self, event: &PushFailure) {
        self.current
            .insert(Self::scope_key(event), Self::head_token(event));
    }

    pub fn ack_is_current(&self, event: &PushFailure) -> bool {
        self.current.get(&Self::scope_key(event)) == Some(&Self::head_token(event))
    }

    pub fn clear_current_ack(&mut self, event: &PushFailure) {
        if self.ack_is_current(event) {
            self.current.remove(&Self::scope_key(event));
        }
    }
}

#[cfg(test)]
mod head_ack_fence_tests {
    use super::*;

    fn event(cid: &str, priority: u64) -> PushFailure {
        PushFailure {
            peer_id: "peer".to_string(),
            doc_id: "doc".to_string(),
            collection_id: "collection".to_string(),
            cid: cid.to_string(),
            head_priority: priority,
            create_retry: false,
            acknowledged: false,
            durable_tx: None,
        }
    }

    #[test]
    fn stale_ack_cannot_clear_a_newer_scope_head() {
        let mut fence = HeadAckFence::default();
        let old = event("old", 1);
        let new = event("new", 2);
        fence.observe_durable(&old);
        fence.observe_durable(&new);

        assert!(!fence.ack_is_current(&old));
        fence.clear_current_ack(&old);
        assert!(fence.ack_is_current(&new));
        fence.clear_current_ack(&new);
        assert!(!fence.ack_is_current(&new));
    }
}

/// Stable diagnostic snapshot of P2P-owned sync resources (#1099).
///
/// Exposed over the P2P operations surface so downstream runtimes can
/// conformance-test and alert on the effective (not just configured) state:
/// live queue occupancy, per-peer backlog, worker slots, pending-DAG depth,
/// retained task handles, and overload counters.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncStatus {
    /// Live queue occupancy and retry/retirement counters.
    pub push_backlog: crate::sync::push_backlog::PushBacklogSnapshot,
    /// Gossip updates folded into a newer update during the short window.
    pub broadcast_coalesced_total: u64,
    /// Duplicate replicator updates folded by the bounded outgoing queue.
    pub push_updates_coalesced_total: u64,
    /// Gossip messages rejected because an unsubscribed sender was configured
    /// only as an outbound replicator target.
    pub gossip_direction_filtered_total: u64,
    pub pending_dags: usize,
    pub pending_dag_capacity: usize,
    pub pending_dag_high_water: u64,
    /// Durable pending-DAG registrations (may exceed `pending_dags`: records
    /// outlive TTL-evicted in-memory entries until their roots merge).
    pub persisted_pending_dags: usize,
    pub persisted_pending_dag_capacity: usize,
    pub persisted_pending_dag_high_water: u64,
    pub pending_resync_in_flight: bool,
    pub retained_background_tasks: usize,
    /// Current/high-water occupancy and terminal overload counters for the
    /// one shared inbound request scheduler.
    pub request_dispatch: crate::sync::DispatchSnapshot,
    /// Current/high-water occupancy of bounded non-authoritative mutation
    /// gossip/artifact work. Durable head markers are installed before this
    /// pool is entered, so shedding here cannot lose a sync obligation.
    pub non_authoritative_broadcast_tasks: usize,
    pub non_authoritative_broadcast_high_water: usize,
    pub non_authoritative_broadcast_rejected_total: u64,
    pub missing_link_retries: u64,
    pub car_requested_cids: u64,
    pub car_present_cids: u64,
    pub car_served_cids: u64,
    pub car_filtered_cids: u64,
    pub provider_rotations: u64,
    pub pending_dag_resolved: u64,
    /// Push-originated missing DAGs durably registered before a success ack.
    pub pending_dag_registered: u64,
    pub pending_dag_expired: u64,
    pub single_flight_suppressed: u64,
    pub already_merged_fast_path: u64,
    pub pending_dag_capacity_shed: u64,
    /// Retry-clock ticks that dispatched a due pending-DAG fetch (#1116 stage 2).
    pub pending_dag_retry_dispatched: u64,
    /// Retry-clock/claim attempts that found no due entry (#1116 stage 2).
    pub pending_dag_retry_suppressed: u64,
    /// Due roots deferred because none of their qualified providers is
    /// currently connected. The existing per-root clock owns the next try.
    pub pending_dag_fetch_deferred_unavailable: u64,
    /// Useful CAR responses coalesced behind an existing local storage owner.
    /// These release the fetch lease without consuming provider-failure attempts.
    pub pending_dag_fetch_deferred_contention: u64,
    /// Roots whose bounded fetch exhausted all attempts/providers.
    pub pending_dag_fetch_exhausted: u64,
    /// Durable pending-DAG obligations discharged by terminal merge/mark.
    pub pending_dag_terminal_merged: u64,
    /// Milliseconds until the earliest due receiver obligation, including a
    /// complete DAG awaiting a terminal merge outcome.
    pub next_pending_retry_in_ms: Option<u64>,
    /// Pending-DAG roots quarantined after a deterministic merge rejection
    /// (#1128); see `SyncManager::quarantine_pending_dag`.
    pub pending_dag_terminal_quarantined: u64,
    /// Current gauge of quarantined pending-DAG roots (#1128).
    pub quarantined_pending_dags: usize,
}

struct SyncShutdownState {
    is_shutting_down: AtomicBool,
    /// Wakes tasks parked in [`SyncShutdownHandle::cancelled`] the moment
    /// shutdown begins, so a periodic loop exits on the signal instead of at
    /// the end of its sleep. Carries no state of its own; `is_shutting_down`
    /// remains the single source of truth and stays a plain atomic because it
    /// is read on hot paths.
    shutdown_notify: Notify,
    shutdown_complete: watch::Receiver<bool>,
    /// Popped by value by the shutdown winner, so a cancelled or panicking
    /// drain still closes the channel by dropping the sender.
    shutdown_complete_tx: SegQueue<watch::Sender<bool>>,
    /// Every retained handle, pending-DAG fetches included; the registry
    /// below keeps only their reservations and abort handles.
    background_tasks: SegQueue<TrackedTask>,
    /// Pruners mid-way through popping and re-pushing live handles; the drain
    /// waits for zero before it takes the queue.
    pruning: AtomicUsize,
    non_authoritative_broadcast_slots: Arc<Semaphore>,
    non_authoritative_broadcast_high_water: AtomicUsize,
    non_authoritative_broadcast_rejected: AtomicU64,
    /// Scheduled and running poll fetches keyed by pending-DAG root. One
    /// registry bounds the event handoff and the retained task, so there is no
    /// hidden pre-semaphore task queue (#1159).
    pending_dag_fetch_task_limit: usize,
    pending_dag_fetch_tasks: Atom<RapidHashMap<Cid, PendingDagFetchTask>>,
}

#[derive(Clone)]
enum PendingDagFetchTask {
    Scheduled,
    Claimed,
    Running(TrackedAbort),
}

impl PendingDagFetchTask {
    fn is_live(&self) -> bool {
        match self {
            Self::Scheduled | Self::Claimed => true,
            Self::Running(task) => !task.is_finished(),
        }
    }
}

const BACKGROUND_TASK_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(500);
const BACKGROUND_TASK_ABORT_TIMEOUT: Duration = Duration::from_secs(5);
const NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT: usize = 32;

/// Shared limiter for poll-based DAG fetches.
///
/// The global semaphore bounds total resource usage, while the per-peer
/// semaphore prevents one source peer from queueing enough fetches to occupy
/// every global permit.
#[derive(Clone)]
pub(crate) struct DagFetchLimiter {
    global: Arc<Semaphore>,
    per_peer: Arc<HopscotchMap<String, Arc<Semaphore>, rapidhash::fast::RandomState>>,
    peer_limit: usize,
}

pub(crate) struct DagFetchPermits {
    _global: OwnedSemaphorePermit,
    _peer: OwnedSemaphorePermit,
}

impl DagFetchLimiter {
    pub(crate) fn new(global_limit: usize) -> Self {
        let global_limit = global_limit.max(1);
        Self {
            global: Arc::new(Semaphore::new(global_limit)),
            per_peer: Arc::new(HopscotchMap::with_hasher(
                rapidhash::fast::RandomState::default(),
            )),
            peer_limit: global_limit.saturating_sub(1).max(1),
        }
    }

    pub(crate) async fn acquire(&self, source_peer: &PeerId) -> Option<DagFetchPermits> {
        let peer_key = source_peer.to_string();
        let peer_semaphore = match self.per_peer.get(&peer_key) {
            Some(semaphore) => semaphore,
            None => self
                .per_peer
                .get_or_insert(peer_key, Arc::new(Semaphore::new(self.peer_limit))),
        };

        let Ok(peer_permit) = peer_semaphore.acquire_owned().await else {
            return None;
        };
        let Ok(global_permit) = self.global.clone().acquire_owned().await else {
            return None;
        };

        Some(DagFetchPermits {
            _global: global_permit,
            _peer: peer_permit,
        })
    }

    fn close(&self) {
        self.global.close();
        for semaphore in self.per_peer.values() {
            semaphore.close();
        }
    }
}

/// Shared shutdown state for coordinator-owned background replication work.
#[derive(Clone)]
pub struct SyncShutdownHandle {
    inner: Arc<SyncShutdownState>,
}

impl SyncShutdownHandle {
    fn new(pending_dag_fetch_task_limit: usize) -> Self {
        let (sender, shutdown_complete) = watch::channel(false);
        let shutdown_complete_tx = SegQueue::new();
        shutdown_complete_tx.push(sender);
        Self {
            inner: Arc::new(SyncShutdownState {
                is_shutting_down: AtomicBool::new(false),
                shutdown_notify: Notify::new(),
                shutdown_complete,
                shutdown_complete_tx,
                background_tasks: SegQueue::new(),
                pruning: AtomicUsize::new(0),
                non_authoritative_broadcast_slots: Arc::new(Semaphore::new(
                    NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT,
                )),
                non_authoritative_broadcast_high_water: AtomicUsize::new(0),
                non_authoritative_broadcast_rejected: AtomicU64::new(0),
                pending_dag_fetch_task_limit: pending_dag_fetch_task_limit.max(1),
                pending_dag_fetch_tasks: Atom::new(RapidHashMap::new()),
            }),
        }
    }

    fn begin_shutdown(&self) -> bool {
        let won = !self.inner.is_shutting_down.swap(true, Ordering::SeqCst);
        if won {
            // Wake every parked `cancelled()` waiter. Ordering matters: the
            // flag is set first, so a waiter that registers between the swap
            // and this call observes the flag and never parks.
            self.inner.shutdown_notify.notify_waiters();
        }
        won
    }

    pub fn is_shutting_down(&self) -> bool {
        self.inner.is_shutting_down.load(Ordering::Acquire)
    }

    /// Resolves as soon as shutdown begins, and immediately if it already has.
    ///
    /// Periodic loops select on this against their sleep so they exit on the
    /// signal rather than at the end of an interval, matching Go's
    /// `select { case <-ctx.Done(): ... }` shape in
    /// `internal/db/p2p/replicator.go`.
    pub async fn cancelled(&self) {
        // Register before observing the flag: a `notify_waiters` that lands
        // after this point wakes us, and one that landed before it is
        // reflected in the flag we are about to read. Checking first would
        // leave a window where neither happens and the caller parks forever.
        let notified = self.inner.shutdown_notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();

        if self.is_shutting_down() {
            return;
        }

        notified.await;
    }

    /// Wait for registered tasks within the graceful and cancellation budgets.
    /// Concurrent callers share teardown; cancelling a caller does not stop it.
    pub async fn shutdown(&self) {
        let mut complete = self.inner.shutdown_complete.clone();
        if self.begin_shutdown() {
            let shutdown = self.clone();
            // The drain owns the only sender, so cancellation or panic closes
            // the channel instead of leaving other shutdown callers parked.
            let sender = self.inner.shutdown_complete_tx.pop();
            n0_future::task::spawn(async move {
                shutdown
                    .drain_background_tasks(BACKGROUND_TASK_SHUTDOWN_TIMEOUT)
                    .await;
                if let Some(sender) = sender {
                    sender.send_replace(true);
                }
            });
        }
        if complete.wait_for(|complete| *complete).await.is_err() {
            tracing::warn!("Coordinator shutdown drain stopped before completing");
        }
    }

    fn spawn_task<F>(&self, future: F) -> bool
    where
        F: std::future::Future<Output = ()> + MaybeSend + 'static,
    {
        if self.is_shutting_down() {
            return false;
        }
        // Retire completed handles on every registration so retained handles
        // track live tasks instead of total spawn count (#1099).
        self.prune_finished_tasks();
        self.register_task(TrackedTask::spawn(future))
    }

    /// Publish the handle, then re-read the shutdown flag behind a full fence.
    /// The drain sets the flag, fences, then takes the queue, so either it
    /// sees this handle or this sees the flag and cancels the task itself.
    fn register_task(&self, task: TrackedTask) -> bool {
        let abort = task.abort_handle();
        self.inner.background_tasks.push(task);
        fence(Ordering::SeqCst);
        if self.inner.is_shutting_down.load(Ordering::SeqCst) {
            abort.abort();
            return false;
        }
        true
    }

    fn prune_finished_tasks(&self) {
        let inner = &self.inner;
        inner.pruning.fetch_add(1, Ordering::SeqCst);
        for _ in 0..inner.background_tasks.len() {
            let Some(task) = inner.background_tasks.pop() else {
                break;
            };
            if !task.is_finished() {
                inner.background_tasks.push(task);
            }
        }
        inner.pruning.fetch_sub(1, Ordering::SeqCst);
    }

    fn try_acquire_non_authoritative_broadcast_slot(&self) -> Option<OwnedSemaphorePermit> {
        let permit =
            match Arc::clone(&self.inner.non_authoritative_broadcast_slots).try_acquire_owned() {
                Ok(permit) => permit,
                Err(_) => {
                    self.inner
                        .non_authoritative_broadcast_rejected
                        .fetch_add(1, Ordering::Relaxed);
                    return None;
                }
            };
        let current = NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT.saturating_sub(
            self.inner
                .non_authoritative_broadcast_slots
                .available_permits(),
        );
        self.inner
            .non_authoritative_broadcast_high_water
            .fetch_max(current, Ordering::Relaxed);
        Some(permit)
    }

    fn non_authoritative_broadcast_stats(&self) -> (usize, usize, u64) {
        (
            NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT.saturating_sub(
                self.inner
                    .non_authoritative_broadcast_slots
                    .available_permits(),
            ),
            self.inner
                .non_authoritative_broadcast_high_water
                .load(Ordering::Relaxed),
            self.inner
                .non_authoritative_broadcast_rejected
                .load(Ordering::Relaxed),
        )
    }

    fn prune_pending_dag_fetches(
        tasks: &RapidHashMap<Cid, PendingDagFetchTask>,
    ) -> RapidHashMap<Cid, PendingDagFetchTask> {
        tasks
            .iter()
            .filter(|(_, task)| task.is_live())
            .map(|(root_cid, task)| (*root_cid, task.clone()))
            .collect()
    }

    fn reserve_pending_dag_fetch(&self, root_cid: Cid) -> bool {
        let mut reserved = false;
        self.inner.pending_dag_fetch_tasks.rcu(|current| {
            let mut next = Self::prune_pending_dag_fetches(current);
            reserved = !self.is_shutting_down()
                && !next.contains_key(&root_cid)
                && next.len() < self.inner.pending_dag_fetch_task_limit;
            if reserved {
                next.insert(root_cid, PendingDagFetchTask::Scheduled);
            }
            next
        });
        reserved
    }

    fn release_pending_dag_fetch_reservation(&self, root_cid: &Cid) {
        self.inner.pending_dag_fetch_tasks.rcu(|current| {
            let mut next = current.clone();
            if matches!(next.get(root_cid), Some(PendingDagFetchTask::Scheduled)) {
                next.remove(root_cid);
            }
            next
        });
    }

    fn available_pending_dag_fetch_slots(&self) -> usize {
        self.inner
            .pending_dag_fetch_task_limit
            .saturating_sub(self.pending_dag_fetch_count())
    }

    /// Live registry entries, after retiring finished fetches. Only publishes
    /// a new registry when something finished.
    fn pending_dag_fetch_count(&self) -> usize {
        let (all_live, len) = self.inner.pending_dag_fetch_tasks.peek(|tasks| {
            (
                tasks.values().all(PendingDagFetchTask::is_live),
                tasks.len(),
            )
        });
        if all_live {
            return len;
        }
        let mut count = 0;
        self.inner.pending_dag_fetch_tasks.rcu(|current| {
            let next = Self::prune_pending_dag_fetches(current);
            count = next.len();
            next
        });
        count
    }

    fn spawn_pending_dag_fetch<F>(&self, root_cid: Cid, future: F) -> bool
    where
        F: std::future::Future<Output = ()> + MaybeSend + 'static,
    {
        let mut claimed = false;
        self.inner.pending_dag_fetch_tasks.rcu(|current| {
            let mut next = Self::prune_pending_dag_fetches(current);
            claimed = !self.is_shutting_down()
                && match next.get(&root_cid) {
                    Some(PendingDagFetchTask::Scheduled) => true,
                    Some(_) => false,
                    None => next.len() < self.inner.pending_dag_fetch_task_limit,
                };
            if claimed {
                next.insert(root_cid, PendingDagFetchTask::Claimed);
            }
            next
        });
        if !claimed {
            return false;
        }

        let shutdown = self.clone();
        let task = TrackedTask::spawn(async move {
            tokio::select! {
                _ = shutdown.cancelled() => {}
                _ = future => {}
            }
        });
        let abort = task.abort_handle();
        self.inner.pending_dag_fetch_tasks.rcu(|current| {
            let mut next = current.clone();
            next.insert(root_cid, PendingDagFetchTask::Running(abort.clone()));
            next
        });
        self.register_task(task)
    }

    /// Number of live retained background task handles. Prunes finished
    /// handles first so a burst of completed tasks does not overstate live
    /// work between registrations.
    pub fn retained_task_count(&self) -> usize {
        self.prune_finished_tasks();
        let reservations = self.inner.pending_dag_fetch_tasks.peek(|tasks| {
            tasks
                .values()
                .filter(|task| !matches!(task, PendingDagFetchTask::Running(_)))
                .count()
        });
        self.inner.background_tasks.len() + reservations
    }

    async fn drain_background_tasks(&self, timeout: Duration) {
        self.inner
            .pending_dag_fetch_tasks
            .store(RapidHashMap::new());
        fence(Ordering::SeqCst);
        while self.inner.pruning.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
        let mut handles = Vec::new();
        while let Some(task) = self.inner.background_tasks.pop() {
            handles.push(task);
        }

        let deadline = n0_future::time::Instant::now() + timeout;
        let mut handles = handles
            .into_iter()
            .map(TrackedTask::into_join_handle)
            .collect::<Vec<_>>()
            .into_iter();
        while let Some(mut handle) = handles.next() {
            let result = n0_future::time::timeout(
                deadline.saturating_duration_since(n0_future::time::Instant::now()),
                &mut handle,
            )
            .await;
            if let Ok(Err(error)) = &result {
                if error.is_panic() {
                    tracing::warn!(%error, "Coordinator background task panicked");
                }
            }
            if result.is_err() {
                tracing::debug!(
                    timeout_ms = timeout.as_millis() as u64,
                    "Coordinator background task exceeded shutdown drain window; aborting remaining tasks"
                );
                handle.abort();
                for pending in handles.as_slice() {
                    pending.abort();
                }
                let remaining = handles.len() + 1;
                let join = async move {
                    for pending in std::iter::once(handle).chain(handles) {
                        if let Err(error) = pending.await {
                            if error.is_panic() {
                                tracing::warn!(%error, "Coordinator background task panicked");
                            }
                        }
                    }
                };
                if n0_future::time::timeout(BACKGROUND_TASK_ABORT_TIMEOUT, join)
                    .await
                    .is_err()
                {
                    tracing::warn!(remaining, "Timed out joining cancelled coordinator tasks");
                }
                return;
            }
        }
    }
}

/// Runtime services and async limits used by coordinator handlers.
pub(super) struct SyncRuntime<T: P2PTransport> {
    /// Transport for sending responses and managing connections.
    pub(super) transport: T,

    /// Broadcaster for publishing updates.
    pub(super) broadcaster: Broadcaster<T>,

    /// Channel for reporting durable retry updates to the host runtime.
    /// Behind a shared slot so the fixed push workers observe a channel that
    /// is installed after construction (`set_failure_channel`).
    pub(super) failure_tx: Arc<AtomOption<tokio::sync::mpsc::Sender<PushFailure>>>,

    /// Limiter for concurrent DAG fetch tasks (configurable via SyncConfig).
    pub(super) dag_fetch_limiter: DagFetchLimiter,

    /// Bounded admission queue for outbound replicator pushes, drained by the
    /// fixed worker pool spawned at construction (#1099).
    pub(super) push_backlog: Arc<super::push_backlog::PushBacklog>,

    pub(super) broadcast_coalescer: Arc<super::broadcast_coalescer::BroadcastCoalescer>,

    /// Temporary per-peer CAR grants scoped to DAGs in active outbound pushes.
    pub(super) selective_car_access: Arc<selective_car_access::SelectiveCarAccess>,

    /// Per-peer rate limiter for gossip dispatch (abuse ladder; drop-only).
    pub(super) rate_limiter: Arc<PeerRateLimiter>,

    /// Per-peer rate limiter for request intake. Refusals are nacked with
    /// `RATE_LIMITED_MESSAGE`; a sender retains its marker and retries on the
    /// durable document ladder.
    pub(super) request_rate_limiter: Arc<PeerRateLimiter>,

    /// Maximum document IDs accepted in a single DocSync request.
    pub(super) max_doc_sync_request_doc_ids: usize,

    /// Shutdown state for coordinator-owned background tasks.
    pub(super) shutdown: SyncShutdownHandle,

    /// Instance-local admission and lifecycle diagnostics for the shared
    /// transport event dispatcher.
    pub(super) dispatch_diagnostics: Arc<crate::sync::DispatchDiagnostics>,

    /// Filter matcher used to evaluate replication filters during push.
    pub(super) filter_matcher: Arc<dyn ReplicationFilterMatcher>,
}

/// Access control and peer identity state for the coordinator.
pub(super) struct SyncAccessState {
    /// Peer state tracker.
    pub(super) peer_state: Arc<PeerStateTracker>,

    /// Local peer ID (for creator field in broadcasts).
    pub(super) local_peer_id: String,

    /// Access control mode.
    pub(super) access_mode: AccessMode,

    /// Replicator registry for access control checks.
    pub(super) replicators: Arc<ReplicatorRegistry>,

    /// Gossip messages rejected by the receive-side direction guard.
    pub(super) gossip_direction_filtered: AtomicU64,
}

/// Collection-ID set shared between the coordinator and its retry tasks.
pub(super) type CollectionSet = HopscotchMap<String, (), rapidhash::fast::RandomState>;

pub(super) fn collection_set() -> Arc<CollectionSet> {
    Arc::new(HopscotchMap::with_hasher(
        rapidhash::fast::RandomState::default(),
    ))
}

/// Subscription and document head support state for the coordinator.
pub(super) struct SyncSubscriptionState {
    /// Serializes durable subscription mutations without blocking readers of
    /// the live installed-topic set while transport I/O is in flight.
    pub(super) mutation: Arc<tokio::sync::Mutex<()>>,

    /// Set of subscribed collection IDs for P2P sync (in-memory cache).
    pub(super) subscribed_collections: Arc<CollectionSet>,

    /// Desired topics with one background installation retry owner.
    pub(super) retrying_subscribes: Arc<CollectionSet>,

    /// Undesired live topics with one background removal retry owner.
    pub(super) retrying_unsubscribes: Arc<CollectionSet>,

    /// Persistent storage for P2P collection subscriptions.
    pub(super) collection_store: Arc<dyn P2PCollectionStorage>,

    /// Document head provider for DocSync responses.
    pub(super) head_provider: Arc<dyn DocumentHeadProvider>,
}

/// Coordinator for P2P synchronization.
///
/// This is the main integration point between the P2P layer and the database.
/// Generic over `T: P2PTransport` to support different transport backends.
pub struct SyncCoordinator<B: Blockstore, T: P2PTransport> {
    /// Runtime services and async coordination primitives.
    pub(super) runtime: SyncRuntime<T>,

    /// Sync manager for block storage.
    pub(super) manager: SyncManager<B>,

    /// Access control and peer identity state.
    pub(super) access: SyncAccessState,

    /// Subscription and doc-sync support state.
    pub(super) subscriptions: SyncSubscriptionState,

    /// Shared peer-authorization backend. Used by both the two-stream
    /// access helpers and the `pubsub_rpc` handlers so both paths make
    /// the same decision for the same peer state.
    pub(super) authorizer: Arc<authorizer::RuntimeAuthorizer<T>>,

    /// CID-aware block classifier shared by Bitswap and CAR serve paths.
    pub(super) classifier: Arc<dyn crate::bitswap::BlockClassifier>,

    /// Late-bound ACP resolver/gate shared by Bitswap and CAR serve paths.
    pub(super) serve_acp: Arc<crate::bitswap::LateBoundServeAcp>,

    /// App replication policy, ANDed with every other replication gate.
    pub(super) replication_policy: Arc<crate::replication_policy::ReplicationPolicyGate>,

    /// Optional document ACP used for local ACP relationship snapshot replay.
    pub(super) document_acp: std::sync::OnceLock<Arc<dyn DocumentACP>>,

    /// KMS pubsub transport. Set by the embedded-node layer when a transport
    /// that supports raw gossip is in use. Left empty otherwise.
    #[cfg(feature = "kms")]
    pub(super) kms_transport: std::sync::OnceLock<Arc<crate::kms::PubsubKeyTransport<T>>>,

    /// Pubsub_rpc DocSync/BranchableSync services (#828). `None` on
    /// transports whose local peer id isn't a libp2p PeerId (e.g. iroh).
    #[cfg(feature = "libp2p-transport")]
    pub(super) pubsub_services: Option<pubsub_services::PubsubServices>,
}

impl<B: Blockstore + 'static, T: P2PTransport> SyncCoordinator<B, T> {
    /// Drain one transport event stream through the shared bounded scheduler.
    pub async fn run_event_dispatcher<E, Handler, HandlerFuture>(
        &self,
        events: tokio::sync::mpsc::Receiver<E>,
        handler: Handler,
    ) where
        E: crate::sync::DispatchEvent + defra_core::thread_bounds::MaybeSend + 'static,
        Handler: Fn(E, crate::sync::DispatchAdmission) -> HandlerFuture
            + Clone
            + defra_core::thread_bounds::MaybeSend
            + 'static,
        HandlerFuture:
            std::future::Future<Output = ()> + defra_core::thread_bounds::MaybeSend + 'static,
    {
        crate::sync::event_dispatcher::run_event_dispatcher(
            events,
            Arc::clone(&self.runtime.dispatch_diagnostics),
            handler,
        )
        .await;
    }

    /// Install the KMS pubsub transport. First-call-wins (OnceLock semantics);
    /// subsequent calls are silently discarded.
    #[cfg(feature = "kms")]
    pub fn install_kms_transport(&self, transport: Arc<crate::kms::PubsubKeyTransport<T>>) {
        let _ = self.kms_transport.set(transport);
    }

    /// Point-in-time snapshot of sync resource state for diagnostics (#1099).
    pub async fn sync_status(&self) -> SyncStatus {
        let diagnostics = self.manager.diagnostics().snapshot();
        let (
            non_authoritative_broadcast_tasks,
            non_authoritative_broadcast_high_water,
            non_authoritative_broadcast_rejected_total,
        ) = self.runtime.shutdown.non_authoritative_broadcast_stats();
        let push_backlog = self.runtime.push_backlog.snapshot().await;
        let push_updates_coalesced_total = push_backlog.coalesced_total;
        SyncStatus {
            push_backlog,
            broadcast_coalesced_total: self.runtime.broadcast_coalescer.coalesced(),
            push_updates_coalesced_total,
            gossip_direction_filtered_total: self
                .access
                .gossip_direction_filtered
                .load(Ordering::Relaxed),
            pending_dags: self.manager.pending_dag_count(),
            pending_dag_capacity: self.manager.max_pending_dags(),
            pending_dag_high_water: diagnostics.pending_dag_high_water,
            persisted_pending_dags: self.manager.persisted_pending_count(),
            persisted_pending_dag_capacity: self.manager.persisted_pending_capacity(),
            persisted_pending_dag_high_water: diagnostics.persisted_pending_dag_high_water,
            pending_resync_in_flight: self.manager.pending_resync_in_flight(),
            retained_background_tasks: self.runtime.shutdown.retained_task_count(),
            request_dispatch: self.runtime.dispatch_diagnostics.snapshot(),
            non_authoritative_broadcast_tasks,
            non_authoritative_broadcast_high_water,
            non_authoritative_broadcast_rejected_total,
            missing_link_retries: diagnostics.missing_link_retries,
            car_requested_cids: diagnostics.car_requested_cids,
            car_present_cids: diagnostics.car_present_cids,
            car_served_cids: diagnostics.car_served_cids,
            car_filtered_cids: diagnostics.car_filtered_cids,
            provider_rotations: diagnostics.provider_rotations,
            pending_dag_resolved: diagnostics.pending_dag_resolved,
            pending_dag_registered: diagnostics.pending_dag_registered,
            pending_dag_expired: diagnostics.pending_dag_expired,
            single_flight_suppressed: diagnostics.single_flight_suppressed,
            already_merged_fast_path: diagnostics.already_merged_fast_path,
            pending_dag_capacity_shed: diagnostics.pending_dag_capacity_shed,
            pending_dag_retry_dispatched: diagnostics.pending_dag_retry_dispatched,
            pending_dag_retry_suppressed: diagnostics.pending_dag_retry_suppressed,
            pending_dag_fetch_deferred_unavailable: diagnostics
                .pending_dag_fetch_deferred_unavailable,
            pending_dag_fetch_deferred_contention: diagnostics
                .pending_dag_fetch_deferred_contention,
            pending_dag_fetch_exhausted: diagnostics.pending_dag_fetch_exhausted,
            pending_dag_terminal_merged: diagnostics.pending_dag_terminal_merged,
            next_pending_retry_in_ms: self.manager.next_pending_retry_in_ms(),
            pending_dag_terminal_quarantined: diagnostics.pending_dag_terminal_quarantined,
            quarantined_pending_dags: self.manager.quarantined_pending_count(),
        }
    }

    /// Install the durable pending-DAG store (#1099). First-call-wins.
    /// Hydrates the durable-cap accounting before returning.
    pub async fn install_pending_dag_store(
        &self,
        store: Arc<dyn crate::sync::pending_store::PendingDagStorage>,
    ) {
        self.manager.install_pending_dag_store(store).await;
    }

    /// Reconcile persisted pending-DAG registrations after restart. Incomplete
    /// roots are restored as immediately due; the receiver retry clock remains
    /// the sole owner that claims and dispatches their fetches. Returns the
    /// restored count.
    pub async fn restore_pending_dags(&self) -> usize {
        self.manager.resync_persisted_pending_dags().await
    }

    /// Periodic bounded drain for durable pending-DAG registrations: sweeps
    /// at `interval` until shutdown, so records skipped at capacity (or whose
    /// in-memory entries TTL-expired) are re-driven even when no peer
    /// reconnects and the node never restarts. The steady-state early-exit
    /// inside the sweep makes idle ticks free. Run from a spawned task.
    pub async fn run_pending_dag_resync(&self, interval: Duration) {
        loop {
            if self.runtime.shutdown.is_shutting_down() {
                return;
            }
            tokio::select! {
                _ = self.runtime.shutdown.cancelled() => return,
                _ = self.manager.resync_persisted_pending_dags() => {}
            }
            tokio::select! {
                _ = n0_future::time::sleep(interval) => {}
                _ = self.runtime.shutdown.cancelled() => return,
            }
        }
    }

    /// The receiver's sole re-arm loop (#1116 stage 2): wake on newly eligible
    /// work or each `interval`, claiming only what the bounded owner can accept.
    /// Registration, partial progress, reconnect, and restart only make roots
    /// due; none of them emits `DagNeedsFetch` independently.
    /// Returns an invalid-configuration error for a zero interval, including
    /// after shutdown. A valid clock returns successfully when shutdown begins.
    pub async fn run_pending_dag_retry_clock(
        &self,
        interval: Duration,
    ) -> crate::error::Result<()> {
        if interval.is_zero() {
            return Err(crate::error::Error::InvalidConfig(
                "pending-DAG retry interval must be greater than zero".into(),
            ));
        }
        let mut retry_tick = n0_future::time::interval(interval);
        retry_tick.set_missed_tick_behavior(n0_future::time::MissedTickBehavior::Skip);
        loop {
            if self.runtime.shutdown.is_shutting_down() {
                return Ok(());
            }
            tokio::select! {
                _ = retry_tick.tick() => {}
                _ = self.manager.pending_dag_ready() => {}
                _ = self.runtime.shutdown.cancelled() => return Ok(()),
            }
            self.dispatch_due_pending_dag_fetches(n0_future::time::Instant::now());
        }
    }

    fn dispatch_due_pending_dag_fetches(&self, now: n0_future::time::Instant) -> usize {
        let due = self.manager.due_pending_dag_retries(now);
        let event_tx = self.manager.event_sender();
        let mut available = self.runtime.shutdown.available_pending_dag_fetch_slots();
        let mut count = 0;
        for (root_cid, dag) in due {
            if available == 0 {
                break;
            }
            if !self.runtime.shutdown.reserve_pending_dag_fetch(root_cid) {
                continue;
            }
            available -= 1;

            let Ok(event_permit) = event_tx.try_reserve() else {
                self.runtime
                    .shutdown
                    .release_pending_dag_fetch_reservation(&root_cid);
                break;
            };
            if !self.manager.try_claim_pending_dag_dispatch(&root_cid, now) {
                self.runtime
                    .shutdown
                    .release_pending_dag_fetch_reservation(&root_cid);
                available += 1;
                continue;
            }

            self.dispatch_pending_dag_fetch(root_cid, &dag, event_permit);
            count += 1;
        }
        count
    }

    #[cfg(test)]
    pub(crate) fn dispatch_due_pending_dag_fetches_for_test(
        &self,
        now: n0_future::time::Instant,
    ) -> usize {
        self.dispatch_due_pending_dag_fetches(now)
    }

    /// Build the provider list for a fetch dispatch from positive per-CID
    /// availability evidence plus the authenticated DAG origin. A newly
    /// connected or root-only peer may expedite the receiver clock, but it
    /// must not become a linked-DAG provider merely by doing so (#1512).
    fn dispatch_pending_dag_fetch(
        &self,
        root_cid: Cid,
        dag: &PendingDag,
        event_permit: tokio::sync::mpsc::Permit<'_, crate::sync::SyncEvent>,
    ) {
        let missing: Vec<_> = dag.missing.iter().copied().collect();
        let mut providers = self.manager.get_providers_for_cids(&missing);
        if let Some(source_peer) = dag.source_peer.clone() {
            if !providers.contains(&source_peer) {
                providers.push(source_peer);
            }
        }
        for provider in dag
            .alternate_providers
            .iter()
            .take(crate::sync::pending_store::MAX_PENDING_DAG_ALTERNATE_PROVIDERS)
        {
            if !providers.contains(provider) {
                providers.push(provider.clone());
            }
        }
        tracing::debug!(
            root_cid = %root_cid,
            missing_count = missing.len(),
            fetch_failures = dag.fetch_failures,
            "Dispatching pending DAG fetch"
        );
        event_permit.send(crate::sync::SyncEvent::DagNeedsFetch {
            root_cid,
            missing,
            providers,
            doc_id: dag.doc_id.clone(),
            collection_id: dag.collection_id.clone(),
            creator: dag.creator.clone(),
            sender_peer: dag.source_peer.clone(),
            is_explicit_replicator: dag.is_explicit_replicator,
            explicit_replay_authorization: dag.explicit_replay_authorization.clone(),
        });
    }

    #[cfg(test)]
    fn dispatch_pending_dag_fetch_for_test(&self, root_cid: Cid, dag: &PendingDag) {
        let event_tx = self.manager.event_sender();
        let event_permit = event_tx
            .try_reserve()
            .expect("test event receiver must have capacity");
        self.dispatch_pending_dag_fetch(root_cid, dag, event_permit);
    }

    pub fn shutdown_handle(&self) -> SyncShutdownHandle {
        self.runtime.shutdown.clone()
    }

    pub async fn shutdown(&self) {
        #[cfg(feature = "libp2p-transport")]
        if let Some(services) = self.pubsub_services.as_ref() {
            services.set_ready(false);
            let cancelled = services.cancel_in_flight();
            if cancelled > 0 {
                tracing::debug!(
                    cancelled,
                    "Cancelled in-flight pubsub_rpc requests during coordinator shutdown"
                );
            }
        }
        self.runtime.dag_fetch_limiter.close();
        self.runtime.push_backlog.close();
        self.runtime.shutdown.shutdown().await;
    }

    /// Spawn work owned by this coordinator so shutdown can drain or cancel it.
    pub fn spawn_background_task<F>(&self, task_name: &'static str, future: F)
    where
        F: std::future::Future<Output = ()> + MaybeSend + 'static,
    {
        if !self.runtime.shutdown.spawn_task(future) {
            tracing::debug!(task = task_name, "Skipping background task during shutdown");
        }
    }

    /// Spawn mutation-adjacent gossip/artifact work in a distinct bounded
    /// pool. Callers must install durable document/collection head markers
    /// before using this method; overflow therefore sheds only redundant,
    /// non-authoritative dissemination work.
    pub fn spawn_non_authoritative_broadcast_task<F>(&self, task_name: &'static str, future: F)
    where
        F: std::future::Future<Output = ()> + MaybeSend + 'static,
    {
        if self.runtime.shutdown.is_shutting_down() {
            tracing::debug!(
                task = task_name,
                "Skipping background broadcast during shutdown"
            );
            return;
        }
        let Some(permit) = self
            .runtime
            .shutdown
            .try_acquire_non_authoritative_broadcast_slot()
        else {
            tracing::warn!(
                task = task_name,
                limit = NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT,
                "Non-authoritative background broadcast pool full; durable head marker retains delivery ownership"
            );
            return;
        };
        self.runtime.shutdown.spawn_task(async move {
            future.await;
            drop(permit);
        });
    }

    /// Like `spawn_non_authoritative_broadcast_task`, but a full pool runs the
    /// future inline instead of dropping it. For work that no durable marker
    /// would replay if it were shed, such as an SE artifact push after a merge.
    pub async fn spawn_or_run_non_authoritative_broadcast<F>(
        &self,
        task_name: &'static str,
        future: F,
    ) where
        F: std::future::Future<Output = ()> + MaybeSend + 'static,
    {
        if self.runtime.shutdown.is_shutting_down() {
            tracing::debug!(
                task = task_name,
                "Skipping background broadcast during shutdown"
            );
            return;
        }
        match self
            .runtime
            .shutdown
            .try_acquire_non_authoritative_broadcast_slot()
        {
            Some(permit) => {
                self.runtime.shutdown.spawn_task(async move {
                    future.await;
                    drop(permit);
                });
            }
            None => {
                tracing::warn!(
                    task = task_name,
                    limit = NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT,
                    "Non-authoritative background broadcast pool full; running inline"
                );
                future.await;
            }
        }
    }

    pub(crate) fn spawn_pending_dag_fetch_task<F>(
        &self,
        root_cid: Cid,
        task_name: &'static str,
        future: F,
    ) -> bool
    where
        F: std::future::Future<Output = ()> + MaybeSend + 'static,
    {
        if self
            .runtime
            .shutdown
            .spawn_pending_dag_fetch(root_cid, future)
        {
            true
        } else {
            self.runtime
                .shutdown
                .release_pending_dag_fetch_reservation(&root_cid);
            self.manager
                .diagnostics()
                .record_pending_dag_retry_suppressed();
            tracing::debug!(
                task = task_name,
                root_cid = %root_cid,
                "Suppressing duplicate pending-DAG fetch task"
            );
            false
        }
    }

    pub(crate) fn release_pending_dag_fetch_reservation(&self, root_cid: &Cid) {
        self.runtime
            .shutdown
            .release_pending_dag_fetch_reservation(root_cid);
    }

    #[cfg(test)]
    pub(crate) fn pending_dag_count(&self) -> usize {
        self.manager.pending_dag_count()
    }
}

/// Type alias for SyncCoordinator using the libp2p transport.
#[cfg(feature = "libp2p-transport")]
pub type Libp2pSyncCoordinator<B> =
    SyncCoordinator<B, crate::host::libp2p_transport::Libp2pTransport>;

/// Type alias for SyncCoordinator using the iroh transport.
#[cfg(feature = "iroh-transport")]
pub type IrohSyncCoordinator<B> = SyncCoordinator<B, crate::iroh::IrohTransport>;

#[cfg(test)]
mod access_tests;
#[cfg(test)]
mod broadcast_pool_tests;

#[cfg(test)]
mod dag_fetch_limiter_tests {
    use super::DagFetchLimiter;
    use crate::transport::PeerId;
    use std::time::Duration;

    #[tokio::test]
    async fn single_peer_cannot_hold_every_global_permit() {
        let limiter = DagFetchLimiter::new(4);
        let flooder = PeerId::new("flooder".to_string());
        let legitimate = PeerId::new("legitimate".to_string());

        let mut flooder_permits = Vec::new();
        for _ in 0..3 {
            flooder_permits.push(limiter.acquire(&flooder).await.expect("flooder permit"));
        }

        assert!(
            n0_future::time::timeout(Duration::from_millis(20), limiter.acquire(&flooder))
                .await
                .is_err(),
            "one peer should be capped below the global limit"
        );

        let legitimate_permit =
            n0_future::time::timeout(Duration::from_millis(20), limiter.acquire(&legitimate))
                .await
                .expect("legitimate peer should get reserved capacity")
                .expect("limiter open");

        drop(legitimate_permit);
        drop(flooder_permits);
    }
}

#[cfg(test)]
#[path = "../../../tests/coordinator/shutdown.rs"]
mod shutdown_completion_tests;

#[cfg(test)]
mod shutdown_tests {
    use super::broadcast::tests::TestTransport;
    use super::{
        SyncCoordinator, SyncShutdownHandle, BACKGROUND_TASK_SHUTDOWN_TIMEOUT,
        NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT,
    };
    use cid::Cid;
    use multihash_codetable::{Code, MultihashDigest};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    struct BlockingResyncStore {
        load_calls: std::sync::atomic::AtomicUsize,
        resync_entered: tokio::sync::Notify,
    }

    impl BlockingResyncStore {
        fn new() -> Self {
            Self {
                load_calls: std::sync::atomic::AtomicUsize::new(0),
                resync_entered: tokio::sync::Notify::new(),
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    impl crate::sync::pending_store::PendingDagStorage for BlockingResyncStore {
        async fn put(
            &self,
            _root_cid: &Cid,
            _record: &crate::sync::pending_store::PersistedPendingDag,
        ) -> crate::Result<()> {
            unreachable!()
        }

        async fn replace_scope_head(
            &self,
            _superseded_root: Option<&Cid>,
            _root_cid: &Cid,
            _record: &crate::sync::pending_store::PersistedPendingDag,
        ) -> crate::Result<()> {
            unreachable!()
        }

        async fn remove(&self, _root_cid: &Cid) -> crate::Result<()> {
            unreachable!()
        }

        async fn load_all(
            &self,
        ) -> crate::Result<Vec<(Cid, crate::sync::pending_store::PersistedPendingDag)>> {
            if self.load_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                return Ok(Vec::new());
            }
            self.resync_entered.notify_one();
            std::future::pending().await
        }

        async fn quarantine(
            &self,
            _root_cid: &Cid,
            _entry: &crate::sync::pending_store::PersistedQuarantinedDag,
        ) -> crate::Result<()> {
            unreachable!()
        }

        async fn is_quarantined(&self, _root_cid: &Cid) -> crate::Result<bool> {
            unreachable!()
        }

        async fn load_quarantined(
            &self,
        ) -> crate::Result<Vec<(Cid, crate::sync::pending_store::PersistedQuarantinedDag)>>
        {
            Ok(Vec::new())
        }

        async fn remove_quarantined(&self, _root_cid: &Cid) -> crate::Result<()> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn shutdown_waits_for_in_flight_background_task_completion() {
        let shutdown = SyncShutdownHandle::new(4);
        let completed = Arc::new(AtomicBool::new(false));
        let completed_for_task = Arc::clone(&completed);

        shutdown.spawn_task(async move {
            n0_future::time::sleep(Duration::from_millis(50)).await;
            completed_for_task.store(true, Ordering::SeqCst);
        });

        shutdown.shutdown().await;

        assert!(
            completed.load(Ordering::SeqCst),
            "shutdown should allow in-flight background tasks to finish"
        );
    }

    #[test]
    fn non_authoritative_broadcast_slots_are_bounded_and_observable() {
        let shutdown = SyncShutdownHandle::new(4);
        let mut permits = Vec::new();
        for _ in 0..NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT {
            permits.push(
                shutdown
                    .try_acquire_non_authoritative_broadcast_slot()
                    .expect("slot within limit"),
            );
        }
        assert!(
            shutdown
                .try_acquire_non_authoritative_broadcast_slot()
                .is_none(),
            "overflow must be actionable instead of allocating another task"
        );
        assert_eq!(
            shutdown.non_authoritative_broadcast_stats(),
            (
                NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT,
                NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT,
                1,
            )
        );
        drop(permits);
        assert_eq!(shutdown.non_authoritative_broadcast_stats().0, 0);
    }

    /// #1099: completed handles must not accumulate for the process lifetime.
    #[tokio::test]
    async fn spawn_task_prunes_finished_handles() {
        let shutdown = SyncShutdownHandle::new(4);
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for _ in 0..50 {
            let completed = Arc::clone(&completed);
            shutdown.spawn_task(async move {
                completed.fetch_add(1, Ordering::SeqCst);
            });
        }

        let deadline = n0_future::time::Instant::now() + Duration::from_secs(2);
        while completed.load(Ordering::SeqCst) < 50 {
            assert!(n0_future::time::Instant::now() < deadline);
            n0_future::time::sleep(Duration::from_millis(5)).await;
        }

        shutdown.spawn_task(async {
            n0_future::time::sleep(Duration::from_secs(5)).await;
        });

        assert!(
            shutdown.retained_task_count() <= 2,
            "finished handles must be pruned on registration, retained {}",
            shutdown.retained_task_count()
        );
        shutdown.shutdown().await;
    }

    /// A retry-clock tick must not retain another multi-minute poll fetch for
    /// a root whose previous fetch is still alive (#1159 production soak).
    #[tokio::test]
    async fn pending_dag_fetches_are_single_flight_per_root() {
        let shutdown = SyncShutdownHandle::new(4);
        let root = Cid::new_v1(0x55, Code::Sha2_256.digest(b"pending-root"));
        let first_release = Arc::new(tokio::sync::Notify::new());
        let first_release_for_task = Arc::clone(&first_release);

        assert!(shutdown.spawn_pending_dag_fetch(root, async move {
            first_release_for_task.notified().await;
        }));
        assert!(
            !shutdown.spawn_pending_dag_fetch(root, async {}),
            "a live fetch must suppress a second task for the same root"
        );
        assert_eq!(shutdown.retained_task_count(), 1);

        first_release.notify_one();
        let deadline = n0_future::time::Instant::now() + Duration::from_secs(2);
        while shutdown.retained_task_count() != 0 {
            assert!(n0_future::time::Instant::now() < deadline);
            tokio::task::yield_now().await;
        }

        assert!(
            shutdown.spawn_pending_dag_fetch(root, async {}),
            "the root must become eligible after its prior fetch finishes"
        );
        shutdown.shutdown().await;
    }

    #[tokio::test]
    async fn scheduled_and_running_pending_fetches_share_one_bound() {
        let shutdown = SyncShutdownHandle::new(2);
        let first = Cid::new_v1(0x55, Code::Sha2_256.digest(b"first"));
        let second = Cid::new_v1(0x55, Code::Sha2_256.digest(b"second"));
        let third = Cid::new_v1(0x55, Code::Sha2_256.digest(b"third"));
        let release = Arc::new(tokio::sync::Notify::new());
        let task_release = Arc::clone(&release);

        assert!(shutdown.reserve_pending_dag_fetch(first));
        assert!(shutdown.spawn_pending_dag_fetch(first, async move {
            task_release.notified().await;
        }));
        assert!(shutdown.reserve_pending_dag_fetch(second));
        assert_eq!(shutdown.available_pending_dag_fetch_slots(), 0);
        assert!(
            !shutdown.reserve_pending_dag_fetch(third),
            "a scheduled event must consume the same bound as a running task"
        );
        assert_eq!(shutdown.retained_task_count(), 2);

        shutdown.release_pending_dag_fetch_reservation(&second);
        assert!(shutdown.reserve_pending_dag_fetch(third));
        release.notify_one();
        shutdown.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_signals_pending_fetches_before_the_drain_timeout() {
        let shutdown = SyncShutdownHandle::new(1);
        let root = Cid::new_v1(0x55, Code::Sha2_256.digest(b"pending-root"));
        assert!(shutdown.spawn_pending_dag_fetch(root, std::future::pending()));
        tokio::task::yield_now().await;

        let started = n0_future::time::Instant::now();
        shutdown.shutdown().await;

        assert!(
            started.elapsed() < BACKGROUND_TASK_SHUTDOWN_TIMEOUT,
            "pending fetch waited for the abort deadline instead of the shutdown signal"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_uses_single_global_budget_for_background_tasks() {
        let shutdown = SyncShutdownHandle::new(4);

        for _ in 0..3 {
            shutdown.spawn_task(async move {
                n0_future::time::sleep(Duration::from_secs(10)).await;
            });
        }

        let started = n0_future::time::Instant::now();
        shutdown.shutdown().await;
        let elapsed = started.elapsed();

        assert_eq!(elapsed, BACKGROUND_TASK_SHUTDOWN_TIMEOUT);
    }

    /// #1309: a periodic loop must exit on the shutdown signal, not at the end
    /// of its sleep. The pending-DAG sweeps used a bare `sleep(interval)`, so a
    /// task spawned before shutdown kept its `Arc<SyncCoordinator>` (and through
    /// it the store) alive for up to the interval: 60s for the resync sweep.
    ///
    /// The interval here is an hour on purpose. Without the cancellation arm
    /// this test cannot pass by waiting; it can only pass by being woken.
    #[tokio::test]
    async fn periodic_loop_exits_on_the_signal_not_the_interval() {
        let shutdown = SyncShutdownHandle::new(4);
        let exited = Arc::new(AtomicBool::new(false));

        let loop_shutdown = shutdown.clone();
        let loop_exited = Arc::clone(&exited);
        let task = n0_future::task::spawn(async move {
            loop {
                if loop_shutdown.is_shutting_down() {
                    break;
                }
                tokio::select! {
                    _ = n0_future::time::sleep(Duration::from_secs(3600)) => {}
                    _ = loop_shutdown.cancelled() => break,
                }
            }
            loop_exited.store(true, Ordering::SeqCst);
        });

        // Let the loop reach its sleep so the wakeup, not the entry check, is
        // what ends it.
        tokio::task::yield_now().await;
        assert!(
            !exited.load(Ordering::SeqCst),
            "loop must still be parked before shutdown is signalled"
        );

        shutdown.shutdown().await;

        n0_future::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("loop must wake on the signal, not wait out its interval")
            .expect("loop task should not panic");
        assert!(exited.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn shutdown_cancels_an_in_flight_pending_resync() {
        let blockstore = Arc::new(blockstore::DefraBlockstore::new(
            Arc::new(storage::RegolithStore::in_memory().expect("in-memory store")),
            true,
        ));
        let (coordinator, _events) = SyncCoordinator::new(
            TestTransport::new(Vec::new()),
            blockstore,
            crate::sync::SyncConfig::default(),
        )
        .await
        .expect("coordinator");
        let pending_store = Arc::new(BlockingResyncStore::new());
        coordinator
            .install_pending_dag_store(pending_store.clone())
            .await;
        let coordinator = Arc::new(coordinator);

        let resync_task = n0_future::task::spawn({
            let coordinator = Arc::clone(&coordinator);
            async move {
                coordinator
                    .run_pending_dag_resync(Duration::from_secs(3600))
                    .await;
            }
        });
        n0_future::time::timeout(
            Duration::from_secs(5),
            pending_store.resync_entered.notified(),
        )
        .await
        .expect("resync did not enter durable storage");
        assert!(coordinator.manager().pending_resync_in_flight());

        coordinator.shutdown().await;
        n0_future::time::timeout(Duration::from_secs(5), resync_task)
            .await
            .expect("resync did not stop on shutdown")
            .expect("resync task should not panic");
        assert!(!coordinator.manager().pending_resync_in_flight());
    }

    #[tokio::test]
    async fn cancelled_returns_immediately_when_shutdown_already_began() {
        let shutdown = SyncShutdownHandle::new(4);
        shutdown.shutdown().await;

        n0_future::time::timeout(Duration::from_secs(5), shutdown.cancelled())
            .await
            .expect("cancelled() must not park once shutdown has begun");
    }

    /// The register-then-check ordering in `cancelled()` is what makes this
    /// pass: a waiter that observed the flag as false must still be woken by
    /// the `notify_waiters` that follows the flag store.
    #[tokio::test]
    async fn cancelled_does_not_miss_a_shutdown_racing_its_registration() {
        for _ in 0..256 {
            let shutdown = SyncShutdownHandle::new(4);
            let waiter_shutdown = shutdown.clone();
            let waiter = n0_future::task::spawn(async move { waiter_shutdown.cancelled().await });

            let signaller = n0_future::task::spawn(async move { shutdown.shutdown().await });

            n0_future::time::timeout(Duration::from_secs(5), waiter)
                .await
                .expect("cancelled() lost the wakeup and parked forever")
                .expect("waiter should not panic");
            signaller.await.expect("signaller should not panic");
        }
    }
}
