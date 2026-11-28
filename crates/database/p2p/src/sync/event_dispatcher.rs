//! One bounded scheduling policy for every transport entrypoint.
//!
//! The transport queue is the sole resident queue. The dispatcher classifies
//! events into fixed admission, recovery-serving, and completion worker sets;
//! lightweight state transitions drain inline. Excess admission requests are
//! rejected in place instead of waiting for a slot and hiding ownership
//! progress behind unrelated work. A CAR serve that finds its lane full waits
//! a bounded time in the drain's own waiting room, off the drain's path, and
//! the freed slot goes to the peer holding the fewest serves
//! ([`ServeAdmission`]). An event's lease is held until its handler returns.

use std::future::Future;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use edge::{EdgeError, TaskSet};
use futures::future::Fuse;
use futures::FutureExt;
use regolith::sync::Semaphore;

use super::car_serve_admission::{ServeAdmission, ServeOffer};
use super::dispatch_queue::{DispatchSource, Lease, Queued};
use crate::transport::TransportEvent;

/// Whether a bounded request was admitted to a worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchAdmission {
    Admitted,
    Saturated,
    /// A refused event that must be kept (a gossip head) waited past its
    /// deadline for a refusal worker; the handler only reports it (#2489).
    Dropped,
}

/// Fixed scheduling classes for all transport entrypoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchClass {
    Inline,
    Admission,
    Recovery,
    Completion,
}

/// Keep the libp2p host and transport-agnostic event enums on one exhaustive
/// scheduling contract.  Both enums intentionally share these variant names;
/// adding a variant to either enum now fails to compile until its ownership
/// class is chosen here. A variant only one enum has is passed by that enum's
/// call as a further completion pattern.
macro_rules! classify_p2p_event {
    (
        $event:expr,
        $event_type:ident,
        $block_sync_progress:ident,
        $block_sync_complete:ident,
        $block_sync_block_received:ident
        $(, $completion:pat)*
    ) => {
        match $event {
            $event_type::CarFetchRequest { .. } => $crate::sync::DispatchClass::Recovery,

            $event_type::PushLogRequest { .. }
            | $event_type::GossipMessage { .. }
            | $event_type::GossipRawMessage { .. }
            | $event_type::TwoStreamRequest { .. }
            | $event_type::DocSyncRequest { .. }
            | $event_type::BranchableSyncRequest { .. }
            | $event_type::SEArtifactsReceived { .. }
            | $event_type::SEQueryRequest { .. }
            | $event_type::ManageRequest { .. }
            | $event_type::ManageQueryRequest { .. } => $crate::sync::DispatchClass::Admission,

            $event_type::PeerConnected(_)
            | $event_type::PeerDisconnected(_)
            | $event_type::$block_sync_progress { .. }
            | $event_type::$block_sync_complete { .. }
            | $event_type::$block_sync_block_received { .. }
            | $event_type::DocSyncReply { .. }
            | $event_type::BranchableSyncReply { .. }
            | $event_type::CarFetchResponse { .. }
            | $event_type::SEQueryReply { .. }
            | $event_type::ManageReply { .. }
            | $event_type::ManageQueryReply { .. }
            $(| $completion)* => $crate::sync::DispatchClass::Completion,

            $event_type::PeerSubscribed { .. }
            | $event_type::PeerUnsubscribed { .. }
            | $event_type::Listening(_) => $crate::sync::DispatchClass::Inline,
        }
    };
}

pub(crate) use classify_p2p_event;

/// Classifies an event once at the shared transport boundary.
pub trait DispatchEvent {
    /// The compact key that counts a request against its requester's share of
    /// the CAR-serve waiting room and active serve count.
    type ServePeer: Eq + Hash + Clone + defra_core::thread_bounds::MaybeSend + 'static;

    fn dispatch_class(&self) -> DispatchClass;

    /// Whether a refusal of this event must be handled rather than dropped:
    /// a gossip head, which no peer re-announces (#2489). A dropped request
    /// closes its stream and the sender retries; a dropped gossip head is lost.
    fn kept_when_refused(&self) -> bool {
        false
    }

    /// The peer a recovery (CAR serve) request came from, which its share of
    /// the serve lane is counted against. `None` shares one anonymous key.
    fn serve_peer(&self) -> Option<Self::ServePeer> {
        None
    }
}

impl<ResponseToken> DispatchEvent for TransportEvent<ResponseToken> {
    type ServePeer = u64;

    fn dispatch_class(&self) -> DispatchClass {
        self.dispatch_class()
    }

    fn kept_when_refused(&self) -> bool {
        matches!(self, TransportEvent::GossipMessage { .. })
    }

    fn serve_peer(&self) -> Option<Self::ServePeer> {
        match self {
            TransportEvent::CarFetchRequest { peer_id, .. } => {
                Some(rapidhash::v3::rapidhash_v3(peer_id.as_str().as_bytes()))
            }
            _ => None,
        }
    }
}

#[cfg(feature = "libp2p-transport")]
impl DispatchEvent for crate::host::HostEvent {
    type ServePeer = u64;

    fn dispatch_class(&self) -> DispatchClass {
        self.dispatch_class()
    }

    fn kept_when_refused(&self) -> bool {
        matches!(self, crate::host::HostEvent::GossipMessage { .. })
    }

    fn serve_peer(&self) -> Option<Self::ServePeer> {
        match self {
            crate::host::HostEvent::CarFetchRequest { peer_id, .. } => {
                Some(rapidhash::v3::rapidhash_v3(peer_id.as_ref().digest()))
            }
            _ => None,
        }
    }
}

const MAX_ACTIVE_REQUESTS: usize = 32;
// CAR serving has its own lane, sized by `SyncConfig::max_active_car_serves`,
// because receiver-owned recovery must not wait behind serialized PushLog
// admission on the provider.
//
// How long a CAR serve waits for a lane slot before it is answered busy. Far
// below the requester's 30 s response deadline, and long enough for a serve
// to finish on a busy hub (#2536).
#[cfg(not(test))]
const CAR_SERVE_WAIT: std::time::Duration = std::time::Duration::from_millis(500);
#[cfg(test)]
const CAR_SERVE_WAIT: std::time::Duration = std::time::Duration::from_millis(200);
const MAX_ACTIVE_REJECTIONS: usize = 8;
// Refused gossip heads are kept durably by their own workers: local work
// only, so the drain may wait for one without waiting on a remote peer.
const MAX_ACTIVE_GOSSIP_REFUSALS: usize = 8;
// How long the drain waits for a gossip refusal worker before it drops the
// head, counted and logged.
#[cfg(not(test))]
const GOSSIP_REFUSAL_WAIT: std::time::Duration = std::time::Duration::from_secs(30);
#[cfg(test)]
const GOSSIP_REFUSAL_WAIT: std::time::Duration = std::time::Duration::from_millis(50);
// Receiver fetch admission is bounded below this value (four by default).
// The extra slots cover terminal and reply events without allowing transport
// tasks to grow with the event stream.
const MAX_ACTIVE_COMPLETIONS: usize = 16;

/// Instance-local diagnostics for the shared transport scheduler.
#[derive(Debug)]
pub struct DispatchDiagnostics {
    recovery_capacity: usize,
    active_requests: AtomicUsize,
    active_requests_high_water: AtomicUsize,
    active_recovery: AtomicUsize,
    active_recovery_high_water: AtomicUsize,
    waiting_recovery: AtomicUsize,
    waiting_recovery_high_water: AtomicUsize,
    recovery_waited_total: AtomicU64,
    recovery_wait_expired_total: AtomicU64,
    active_rejections: AtomicUsize,
    active_rejections_high_water: AtomicUsize,
    active_completions: AtomicUsize,
    active_completions_high_water: AtomicUsize,
    saturated_total: AtomicU64,
    admission_saturated_total: AtomicU64,
    recovery_saturated_total: AtomicU64,
    rejection_dropped_total: AtomicU64,
    gossip_refusals_dropped_total: AtomicU64,
    #[cfg(test)]
    serve_deadline_arms: AtomicUsize,
}

impl Default for DispatchDiagnostics {
    fn default() -> Self {
        Self::new(crate::sync::DEFAULT_MAX_ACTIVE_CAR_SERVES)
    }
}

/// Stable point-in-time scheduler diagnostics exposed through sync status.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct DispatchSnapshot {
    pub request_capacity: usize,
    pub active_requests: usize,
    pub active_requests_high_water: usize,
    /// CAR serves run at once (`SyncConfig::max_active_car_serves`).
    pub recovery_capacity: usize,
    pub active_recovery: usize,
    pub active_recovery_high_water: usize,
    /// CAR serves waiting for a lane slot.
    pub waiting_recovery: usize,
    pub waiting_recovery_high_water: usize,
    /// CAR serves that found the lane full, waited, and were served.
    pub recovery_waited_total: u64,
    /// CAR serves answered busy because their wait for a slot ran out.
    pub recovery_wait_expired_total: u64,
    pub rejection_capacity: usize,
    pub active_rejections: usize,
    pub active_rejections_high_water: usize,
    pub completion_capacity: usize,
    pub active_completions: usize,
    pub active_completions_high_water: usize,
    /// Sum of `admission_saturated_total` and `recovery_saturated_total`.
    pub saturated_total: u64,
    /// Admission requests refused with a capacity nack.
    pub admission_saturated_total: u64,
    /// CAR serves answered busy: the lane and its waiting room were full, or
    /// the wait for a slot ran out.
    pub recovery_saturated_total: u64,
    pub rejection_dropped_total: u64,
    /// Refused gossip heads dropped because no refusal worker freed in time.
    pub gossip_refusals_dropped_total: u64,
}

impl DispatchDiagnostics {
    /// Diagnostics for a dispatcher whose CAR-serve lane runs
    /// `recovery_capacity` serves at once (at least one).
    pub fn new(recovery_capacity: usize) -> Self {
        Self {
            recovery_capacity: recovery_capacity.max(1),
            active_requests: AtomicUsize::new(0),
            active_requests_high_water: AtomicUsize::new(0),
            active_recovery: AtomicUsize::new(0),
            active_recovery_high_water: AtomicUsize::new(0),
            waiting_recovery: AtomicUsize::new(0),
            waiting_recovery_high_water: AtomicUsize::new(0),
            recovery_waited_total: AtomicU64::new(0),
            recovery_wait_expired_total: AtomicU64::new(0),
            active_rejections: AtomicUsize::new(0),
            active_rejections_high_water: AtomicUsize::new(0),
            active_completions: AtomicUsize::new(0),
            active_completions_high_water: AtomicUsize::new(0),
            saturated_total: AtomicU64::new(0),
            admission_saturated_total: AtomicU64::new(0),
            recovery_saturated_total: AtomicU64::new(0),
            rejection_dropped_total: AtomicU64::new(0),
            gossip_refusals_dropped_total: AtomicU64::new(0),
            #[cfg(test)]
            serve_deadline_arms: AtomicUsize::new(0),
        }
    }

    #[cfg(test)]
    fn note_serve_deadline_armed(&self) {
        self.serve_deadline_arms.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(test)]
    fn serve_deadline_arms(&self) -> usize {
        self.serve_deadline_arms.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self) -> DispatchSnapshot {
        DispatchSnapshot {
            request_capacity: MAX_ACTIVE_REQUESTS,
            active_requests: self.active_requests.load(Ordering::Relaxed),
            active_requests_high_water: self.active_requests_high_water.load(Ordering::Relaxed),
            recovery_capacity: self.recovery_capacity,
            active_recovery: self.active_recovery.load(Ordering::Relaxed),
            active_recovery_high_water: self.active_recovery_high_water.load(Ordering::Relaxed),
            waiting_recovery: self.waiting_recovery.load(Ordering::Relaxed),
            waiting_recovery_high_water: self.waiting_recovery_high_water.load(Ordering::Relaxed),
            recovery_waited_total: self.recovery_waited_total.load(Ordering::Relaxed),
            recovery_wait_expired_total: self.recovery_wait_expired_total.load(Ordering::Relaxed),
            rejection_capacity: MAX_ACTIVE_REJECTIONS,
            active_rejections: self.active_rejections.load(Ordering::Relaxed),
            active_rejections_high_water: self.active_rejections_high_water.load(Ordering::Relaxed),
            completion_capacity: MAX_ACTIVE_COMPLETIONS,
            active_completions: self.active_completions.load(Ordering::Relaxed),
            active_completions_high_water: self
                .active_completions_high_water
                .load(Ordering::Relaxed),
            saturated_total: self.saturated_total.load(Ordering::Relaxed),
            admission_saturated_total: self.admission_saturated_total.load(Ordering::Relaxed),
            recovery_saturated_total: self.recovery_saturated_total.load(Ordering::Relaxed),
            rejection_dropped_total: self.rejection_dropped_total.load(Ordering::Relaxed),
            gossip_refusals_dropped_total: self
                .gossip_refusals_dropped_total
                .load(Ordering::Relaxed),
        }
    }

    fn set_waiting_recovery(&self, waiting: usize) {
        self.waiting_recovery.store(waiting, Ordering::Relaxed);
        self.waiting_recovery_high_water
            .fetch_max(waiting, Ordering::Relaxed);
    }

    fn enter(self: &Arc<Self>, kind: DispatchActivityKind) -> DispatchActivity {
        let (current, high_water) = match kind {
            DispatchActivityKind::Request => {
                (&self.active_requests, &self.active_requests_high_water)
            }
            DispatchActivityKind::Recovery => {
                (&self.active_recovery, &self.active_recovery_high_water)
            }
            DispatchActivityKind::Rejection => {
                (&self.active_rejections, &self.active_rejections_high_water)
            }
            DispatchActivityKind::Completion => (
                &self.active_completions,
                &self.active_completions_high_water,
            ),
        };
        let occupancy = current.fetch_add(1, Ordering::Relaxed) + 1;
        high_water.fetch_max(occupancy, Ordering::Relaxed);
        DispatchActivity {
            diagnostics: Arc::clone(self),
            kind,
        }
    }
}

#[derive(Clone, Copy)]
enum DispatchActivityKind {
    Request,
    Recovery,
    Rejection,
    Completion,
}

struct DispatchActivity {
    diagnostics: Arc<DispatchDiagnostics>,
    kind: DispatchActivityKind,
}

impl Drop for DispatchActivity {
    fn drop(&mut self) {
        let current = match self.kind {
            DispatchActivityKind::Request => &self.diagnostics.active_requests,
            DispatchActivityKind::Recovery => &self.diagnostics.active_recovery,
            DispatchActivityKind::Rejection => &self.diagnostics.active_rejections,
            DispatchActivityKind::Completion => &self.diagnostics.active_completions,
        };
        current.fetch_sub(1, Ordering::Relaxed);
    }
}

/// A CAR-serve request and its lease, as it waits in the serve lane.
type ServeRequest<E> = (E, Option<Lease>);

/// Returns a CAR-serve slot to the drain when its serve ends, a panic or a
/// cancellation included, so the lane never loses a slot.
struct ServeSlot<K: 'static> {
    peer: Option<K>,
    released: regolith::sync::UnboundedSender<K>,
}

impl<K: 'static> Drop for ServeSlot<K> {
    fn drop(&mut self) {
        if let Some(peer) = self.peer.take() {
            let _ = self.released.try_send(peer);
        }
    }
}

type ServeSleep = Fuse<defra_core::thread_bounds::MaybeBoxFuture<'static, ()>>;

/// Keeps one sleep future armed while the earliest waiting deadline is
/// unchanged, so draining other events does not rebuild it every iteration.
struct ServeWait {
    deadline: Option<crate::rt::time::Instant>,
    sleep: Option<ServeSleep>,
}

impl ServeWait {
    fn new() -> Self {
        Self {
            deadline: None,
            sleep: None,
        }
    }

    fn sync(
        &mut self,
        deadline: Option<crate::rt::time::Instant>,
        _diagnostics: &DispatchDiagnostics,
    ) {
        if self.deadline == deadline {
            return;
        }
        self.deadline = deadline;
        self.sleep = deadline.map(|deadline| {
            let sleep: defra_core::thread_bounds::MaybeBoxFuture<'static, ()> =
                Box::pin(crate::rt::time::sleep_until(deadline));
            sleep.fuse()
        });
        #[cfg(test)]
        if deadline.is_some() {
            _diagnostics.note_serve_deadline_armed();
        }
    }

    async fn wait(&mut self) {
        match &mut self.sleep {
            Some(sleep) => sleep.await,
            None => std::future::pending().await,
        }
    }
}

/// Drain transport events without ever waiting for a request-worker slot.
pub(crate) async fn run_event_dispatcher<E, Source, Handler, HandlerFuture>(
    mut events: Source,
    edge: edge::Edge,
    diagnostics: Arc<DispatchDiagnostics>,
    handler: Handler,
) where
    E: DispatchEvent + defra_core::thread_bounds::MaybeSend + 'static,
    Source: DispatchSource<E>,
    Handler: Fn(E, DispatchAdmission) -> HandlerFuture
        + Clone
        + defra_core::thread_bounds::MaybeSend
        + 'static,
    HandlerFuture: Future<Output = ()> + defra_core::thread_bounds::MaybeSend + 'static,
{
    let request_slots = Arc::new(Semaphore::new(MAX_ACTIVE_REQUESTS));
    let mut serves: ServeAdmission<
        Option<E::ServePeer>,
        ServeRequest<E>,
        crate::rt::time::Instant,
    > = ServeAdmission::new(diagnostics.recovery_capacity);
    let (serve_released, serve_releases) = regolith::sync::unbounded::<Option<E::ServePeer>>();
    let rejection_slots = Arc::new(Semaphore::new(MAX_ACTIVE_REJECTIONS));
    let gossip_refusal_slots = Arc::new(regolith::sync::Semaphore::new(MAX_ACTIVE_GOSSIP_REFUSALS));
    let completion_slots = Arc::new(Semaphore::new(MAX_ACTIVE_COMPLETIONS));
    let mut tasks = TaskSet::new(edge);
    let mut serve_wait = ServeWait::new();

    loop {
        serve_wait.sync(serves.next_deadline(), &diagnostics);
        futures::select_biased! {
            result = if tasks.is_empty() {
                Fuse::terminated()
            } else {
                tasks.join_next().fuse()
            } => {
                report_join_result(result.expect("dispatcher task set was non-empty"));
            }
            peer = serve_releases.recv_async().fuse() => {
                let Some(peer) = peer else { continue };
                if let Some((peer, (event, lease))) = serves.release(&peer) {
                    diagnostics.recovery_waited_total.fetch_add(1, Ordering::Relaxed);
                    spawn_serve(&mut tasks, &diagnostics, &handler, &serve_released, peer, event, lease);
                }
                diagnostics.set_waiting_recovery(serves.waiting());
            }
            _ = serve_wait.wait().fuse() => {
                let now = crate::rt::time::Instant::now();
                while let Some((event, lease)) = serves.pop_expired(now) {
                    diagnostics.recovery_wait_expired_total.fetch_add(1, Ordering::Relaxed);
                    refuse(
                        &mut tasks,
                        &diagnostics,
                        &handler,
                        &rejection_slots,
                        &diagnostics.recovery_saturated_total,
                        event,
                        lease,
                    );
                }
                diagnostics.set_waiting_recovery(serves.waiting());
            }
            queued = events.next(completion_slots.available_permits() > 0).fuse() => {
                let Some(Queued { event, lease }) = queued else {
                    break;
                };
                let class = event.dispatch_class();
                if class == DispatchClass::Completion {
                    // A per-class source only yields a completion when a slot
                    // is free. A single-queue source can hand one over while
                    // the set is full; then wait for an owner to finish before
                    // retaining more work outside the transport queue.
                    let permit = completion_slots.acquire_owned(1).await;
                    let completion_handler = handler.clone();
                    let activity = diagnostics.enter(DispatchActivityKind::Completion);
                    tasks.spawn(async move {
                        let _permit = permit;
                        let _activity = activity;
                        let _lease = lease;
                        completion_handler(event, DispatchAdmission::Admitted).await;
                    });
                } else if class == DispatchClass::Inline {
                    handler.clone()(event, DispatchAdmission::Admitted).await;
                    drop(lease);
                } else if class == DispatchClass::Recovery {
                    let peer = event.serve_peer();
                    let deadline = crate::rt::time::Instant::now() + CAR_SERVE_WAIT;
                    match serves.offer(peer.clone(), (event, lease), deadline) {
                        ServeOffer::Admitted((event, lease)) => {
                            spawn_serve(
                                &mut tasks,
                                &diagnostics,
                                &handler,
                                &serve_released,
                                peer,
                                event,
                                lease,
                            );
                        }
                        ServeOffer::Waiting => diagnostics.set_waiting_recovery(serves.waiting()),
                        ServeOffer::Shed((event, lease)) => refuse(
                            &mut tasks,
                            &diagnostics,
                            &handler,
                            &rejection_slots,
                            &diagnostics.recovery_saturated_total,
                            event,
                            lease,
                        ),
                    }
                } else if let Some(permit) = request_slots.acquire_owned(1).now_or_never() {
                    let task_handler = handler.clone();
                    let activity = diagnostics.enter(DispatchActivityKind::Request);
                    tasks.spawn(async move {
                        let _permit = permit;
                        let _activity = activity;
                        let _lease = lease;
                        task_handler(event, DispatchAdmission::Admitted).await;
                    });
                } else if event.kept_when_refused() {
                    diagnostics.saturated_total.fetch_add(1, Ordering::Relaxed);
                    // A refused gossip head is kept durably on its own
                    // workers. The drain waits for one rather than drop
                    // the head: no peer re-announces it (#2489). Waiting
                    // holds back the bounded transport queue.
                    let waited = n0_future::time::timeout(
                        GOSSIP_REFUSAL_WAIT,
                        gossip_refusal_slots.acquire_owned(1),
                    )
                    .await;
                    match waited {
                        Ok(permit) => {
                            let refusal_handler = handler.clone();
                            let activity = diagnostics.enter(DispatchActivityKind::Rejection);
                            tasks.spawn(async move {
                                let _permit = permit;
                                let _activity = activity;
                                let _lease = lease;
                                refusal_handler(event, DispatchAdmission::Saturated).await;
                            });
                        }
                        Err(_) => {
                            diagnostics
                                .gossip_refusals_dropped_total
                                .fetch_add(1, Ordering::Relaxed);
                            tracing::warn!(
                                wait_ms = GOSSIP_REFUSAL_WAIT.as_millis() as u64,
                                "Dropped a refused gossip head: no refusal worker freed in time"
                            );
                            // The report names the peer and root as a node
                            // event. At most one per deadline: the drain
                            // waited the whole deadline for this one.
                            let report_handler = handler.clone();
                            tasks.spawn(async move {
                                let _lease = lease;
                                report_handler(event, DispatchAdmission::Dropped).await;
                            });
                        }
                    }
                } else {
                    refuse(
                        &mut tasks,
                        &diagnostics,
                        &handler,
                        &rejection_slots,
                        &diagnostics.admission_saturated_total,
                        event,
                        lease,
                    );
                }
            }
        }
    }

    tasks.abort_all();
    while let Some(result) = tasks.join_next().await {
        report_join_result(result);
    }
}

/// Run an admitted CAR serve; its [`ServeSlot`] hands the slot back to the
/// drain when the handler returns.
fn spawn_serve<E, K, Handler, HandlerFuture>(
    tasks: &mut TaskSet<()>,
    diagnostics: &Arc<DispatchDiagnostics>,
    handler: &Handler,
    released: &regolith::sync::UnboundedSender<K>,
    peer: K,
    event: E,
    lease: Option<Lease>,
) where
    E: defra_core::thread_bounds::MaybeSend + 'static,
    K: defra_core::thread_bounds::MaybeSend + 'static,
    Handler: Fn(E, DispatchAdmission) -> HandlerFuture + Clone,
    HandlerFuture: Future<Output = ()> + defra_core::thread_bounds::MaybeSend + 'static,
{
    let slot = ServeSlot {
        peer: Some(peer),
        released: released.clone(),
    };
    let activity = diagnostics.enter(DispatchActivityKind::Recovery);
    let serve = handler.clone()(event, DispatchAdmission::Admitted);
    tasks.spawn(async move {
        let _slot = slot;
        let _activity = activity;
        let _lease = lease;
        serve.await;
    });
}

/// Answer a request its lane could not take, counted against `lane_saturated`.
///
/// A capacity nack is best effort: the remote peer may stop reading its
/// response stream. Keep those writes off the sole event drain. A stalled
/// write keeps its bounded slot; subsequent excess tokens are dropped, closing
/// the stream as an actionable transport failure for durable retry.
fn refuse<E, Handler, HandlerFuture>(
    tasks: &mut TaskSet<()>,
    diagnostics: &Arc<DispatchDiagnostics>,
    handler: &Handler,
    rejection_slots: &Arc<Semaphore>,
    lane_saturated: &AtomicU64,
    event: E,
    lease: Option<Lease>,
) where
    E: defra_core::thread_bounds::MaybeSend + 'static,
    Handler: Fn(E, DispatchAdmission) -> HandlerFuture + Clone,
    HandlerFuture: Future<Output = ()> + defra_core::thread_bounds::MaybeSend + 'static,
{
    diagnostics.saturated_total.fetch_add(1, Ordering::Relaxed);
    lane_saturated.fetch_add(1, Ordering::Relaxed);
    if let Some(permit) = rejection_slots.acquire_owned(1).now_or_never() {
        let activity = diagnostics.enter(DispatchActivityKind::Rejection);
        let rejection = handler.clone()(event, DispatchAdmission::Saturated);
        tasks.spawn(async move {
            let _permit = permit;
            let _activity = activity;
            let _lease = lease;
            rejection.await;
        });
    } else {
        diagnostics
            .rejection_dropped_total
            .fetch_add(1, Ordering::Relaxed);
        tracing::debug!("dropping transport request after rejection workers saturated");
    }
}

fn report_join_result(result: Result<(), EdgeError>) {
    match result {
        Ok(()) | Err(EdgeError::Cancelled) => {}
        Err(EdgeError::Closed) => {
            tracing::debug!("transport request task refused: the network is shutting down");
        }
        Err(error) => tracing::error!(%error, "transport request task failed"),
    }
}

#[cfg(test)]
#[path = "event_dispatcher_serve_tests.rs"]
mod serve_tests;

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use nuclei::sync::{mpsc, Semaphore};
    use regolith::sync::SegQueue;

    use super::*;

    #[test]
    fn remote_raw_gossip_is_admission_and_peer_lifecycle_is_completion() {
        let raw = crate::transport::TransportEvent::<()>::GossipRawMessage {
            propagation_source: crate::transport::PeerId::new("peer".to_string()),
            source: crate::transport::PeerId::new("peer".to_string()),
            message_id: crate::transport::MessageId::new("message".to_string()),
            topic: "topic".to_string(),
            data: Vec::new(),
        };
        assert_eq!(raw.dispatch_class(), DispatchClass::Admission);

        let connected = crate::transport::TransportEvent::<()>::PeerConnected(
            crate::transport::PeerId::new("peer".to_string()),
        );
        assert_eq!(connected.dispatch_class(), DispatchClass::Completion);

        let listening = crate::transport::TransportEvent::<()>::Listening(
            crate::transport::PeerAddr::new("address".to_string()),
        );
        assert_eq!(listening.dispatch_class(), DispatchClass::Inline);
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TestEvent {
        Request(usize),
        Recovery(usize),
        Completion,
        Gossip(usize),
    }

    impl DispatchEvent for TestEvent {
        type ServePeer = ();

        fn dispatch_class(&self) -> DispatchClass {
            match self {
                Self::Request(_) | Self::Gossip(_) => DispatchClass::Admission,
                Self::Recovery(_) => DispatchClass::Recovery,
                Self::Completion => DispatchClass::Completion,
            }
        }

        fn kept_when_refused(&self) -> bool {
            matches!(self, Self::Gossip(_))
        }
    }

    #[test]
    fn a_head_with_no_refusal_worker_past_the_deadline_is_handed_over_as_dropped() {
        let edge = edge::testing::edge();
        edge.block_on(async {
            let (tx, rx) = crate::sync::dispatch_queue::dispatch_queue::<TestEvent>(256);
            let diagnostics = Arc::new(DispatchDiagnostics::default());
            // Every request worker and every gossip refusal worker stays busy.
            let (busy_tx, busy) = regolith::sync::unbounded::<()>();
            let (dropped_tx, dropped) = regolith::sync::unbounded::<usize>();
            let dispatcher = crate::rt::spawn(run_event_dispatcher(
                rx,
                edge.clone(),
                Arc::clone(&diagnostics),
                move |event, admission| {
                    let busy = busy.clone();
                    let dropped_tx = dropped_tx.clone();
                    async move {
                        match (event, admission) {
                            (TestEvent::Gossip(index), DispatchAdmission::Dropped) => {
                                let _ = dropped_tx.try_send(index);
                            }
                            (TestEvent::Request(_) | TestEvent::Gossip(_), _) => {
                                let _ = busy.recv_async().await;
                            }
                            _ => {}
                        }
                    }
                },
            ));
            for index in 0..MAX_ACTIVE_REQUESTS {
                tx.send(TestEvent::Request(index)).await.unwrap();
            }
            for index in 0..=MAX_ACTIVE_GOSSIP_REFUSALS {
                tx.send(TestEvent::Gossip(index)).await.unwrap();
            }
            assert_eq!(
                dropped.recv_async().await.unwrap(),
                MAX_ACTIVE_GOSSIP_REFUSALS,
                "the head past every refusal worker is handed over to be reported"
            );
            assert_eq!(diagnostics.snapshot().gossip_refusals_dropped_total, 1);
            drop(busy_tx);
            drop(tx);
            dispatcher.join().await.unwrap();
        })
        .unwrap();
    }

    #[test]
    fn refused_gossip_waits_for_a_refusal_worker_instead_of_being_dropped() {
        const HEADS: usize = 3 * MAX_ACTIVE_GOSSIP_REFUSALS;
        let edge = edge::testing::edge();
        edge.block_on(async {
            let (tx, rx) = crate::sync::dispatch_queue::dispatch_queue::<TestEvent>(256);
            let diagnostics = Arc::new(DispatchDiagnostics::default());
            // Every request worker and every nack worker stays busy.
            let (busy_tx, busy) = regolith::sync::unbounded::<()>();
            let (refused_tx, refused) = regolith::sync::unbounded::<usize>();
            let dispatcher = crate::rt::spawn(run_event_dispatcher(
                rx,
                edge.clone(),
                Arc::clone(&diagnostics),
                move |event, admission| {
                    let busy = busy.clone();
                    let refused_tx = refused_tx.clone();
                    async move {
                        match (event, admission) {
                            (TestEvent::Request(_), _) => {
                                let _ = busy.recv_async().await;
                            }
                            (TestEvent::Gossip(index), DispatchAdmission::Saturated) => {
                                let _ = refused_tx.try_send(index);
                            }
                            _ => {}
                        }
                    }
                },
            ));
            for index in 0..MAX_ACTIVE_REQUESTS + MAX_ACTIVE_REJECTIONS {
                tx.send(TestEvent::Request(index)).await.unwrap();
            }
            for index in 0..HEADS {
                tx.send(TestEvent::Gossip(index)).await.unwrap();
            }
            let mut kept = Vec::new();
            for _ in 0..HEADS {
                kept.push(refused.recv_async().await.unwrap());
            }
            kept.sort_unstable();
            assert_eq!(
                kept,
                (0..HEADS).collect::<Vec<_>>(),
                "every refused head reaches a worker"
            );
            let snapshot = diagnostics.snapshot();
            assert_eq!(snapshot.gossip_refusals_dropped_total, 0);
            assert_eq!(snapshot.rejection_dropped_total, 0);
            drop(busy_tx);
            drop(tx);
            dispatcher.join().await.unwrap();
        })
        .unwrap();
    }

    #[nuclei::test(threads = 0)]
    async fn saturation_rejects_work_and_still_drains_completion() {
        let (tx, rx) = mpsc::channel(64);
        let observed = Arc::new(SegQueue::new());
        let started = Arc::new(Semaphore::new(0));
        let release = Arc::new(Semaphore::new(0));
        let diagnostics = Arc::new(DispatchDiagnostics::default());

        let dispatcher = {
            let observed = Arc::clone(&observed);
            let started = Arc::clone(&started);
            let release = Arc::clone(&release);
            crate::rt::spawn(run_event_dispatcher(
                rx,
                edge::testing::edge(),
                Arc::clone(&diagnostics),
                move |event, admission| {
                    let observed = Arc::clone(&observed);
                    let started = Arc::clone(&started);
                    let release = Arc::clone(&release);
                    async move {
                        observed.push((event, admission));
                        if matches!(event, TestEvent::Request(index) if index < MAX_ACTIVE_REQUESTS)
                            && admission == DispatchAdmission::Admitted
                        {
                            started.add_permits(1);
                            let _permit = release.acquire().await.unwrap();
                        }
                    }
                },
            ))
        };

        for index in 0..MAX_ACTIVE_REQUESTS {
            tx.send(TestEvent::Request(index)).await.unwrap();
        }
        tx.send(TestEvent::Request(MAX_ACTIVE_REQUESTS))
            .await
            .unwrap();
        tx.send(TestEvent::Completion).await.unwrap();

        crate::rt::time::timeout(
            std::time::Duration::from_secs(1),
            started.acquire_many(MAX_ACTIVE_REQUESTS as u32),
        )
        .await
        .expect("the request worker bound should fill")
        .unwrap()
        .forget();

        crate::rt::time::timeout(std::time::Duration::from_secs(1), async {
            let mut events = Vec::new();
            loop {
                while let Some(event) = observed.pop() {
                    events.push(event);
                }
                let rejected = events.contains(&(
                    TestEvent::Request(MAX_ACTIVE_REQUESTS),
                    DispatchAdmission::Saturated,
                ));
                let completed =
                    events.contains(&(TestEvent::Completion, DispatchAdmission::Admitted));
                if rejected && completed {
                    break;
                }
                crate::rt::yield_now().await;
            }
        })
        .await
        .expect("saturation must not hide a completion");
        let snapshot = diagnostics.snapshot();
        assert_eq!(snapshot.admission_saturated_total, 1);
        assert_eq!(snapshot.recovery_saturated_total, 0);
        assert_eq!(snapshot.saturated_total, 1);

        release.add_permits(MAX_ACTIVE_REQUESTS);
        drop(tx);
        dispatcher.join().await.unwrap();
    }

    #[nuclei::test(threads = 0)]
    async fn saturated_admission_does_not_block_recovery_serving() {
        let (tx, rx) = mpsc::channel(64);
        let requests_started = Arc::new(Semaphore::new(0));
        let release_requests = Arc::new(Semaphore::new(0));
        let recovery_observed = Arc::new(Semaphore::new(0));
        let diagnostics = Arc::new(DispatchDiagnostics::default());

        let dispatcher = {
            let requests_started = Arc::clone(&requests_started);
            let release_requests = Arc::clone(&release_requests);
            let recovery_observed = Arc::clone(&recovery_observed);
            crate::rt::spawn(run_event_dispatcher(
                rx,
                edge::testing::edge(),
                Arc::clone(&diagnostics),
                move |event, admission| {
                    let requests_started = Arc::clone(&requests_started);
                    let release_requests = Arc::clone(&release_requests);
                    let recovery_observed = Arc::clone(&recovery_observed);
                    async move {
                        match (event, admission) {
                            (TestEvent::Request(_), DispatchAdmission::Admitted) => {
                                requests_started.add_permits(1);
                                let _permit = release_requests.acquire().await.unwrap();
                            }
                            (TestEvent::Recovery(_), DispatchAdmission::Admitted) => {
                                recovery_observed.add_permits(1);
                            }
                            _ => {}
                        }
                    }
                },
            ))
        };

        for index in 0..MAX_ACTIVE_REQUESTS {
            tx.send(TestEvent::Request(index)).await.unwrap();
        }
        requests_started
            .acquire_many(MAX_ACTIVE_REQUESTS as u32)
            .await
            .unwrap()
            .forget();
        tx.send(TestEvent::Recovery(0)).await.unwrap();

        crate::rt::time::timeout(Duration::from_secs(1), recovery_observed.acquire())
            .await
            .expect("ownership admission saturation must not block CAR serving")
            .unwrap()
            .forget();
        let snapshot = diagnostics.snapshot();
        assert_eq!(snapshot.active_requests, MAX_ACTIVE_REQUESTS);
        assert_eq!(snapshot.active_recovery_high_water, 1);
        assert_eq!(snapshot.recovery_saturated_total, 0);

        release_requests.add_permits(MAX_ACTIVE_REQUESTS);
        drop(tx);
        dispatcher.join().await.unwrap();
    }

    #[nuclei::test(threads = 0)]
    async fn stalled_capacity_rejection_does_not_block_completion() {
        let (tx, rx) = mpsc::channel(64);
        let started = Arc::new(Semaphore::new(0));
        let release_requests = Arc::new(Semaphore::new(0));
        let release_rejection = Arc::new(Semaphore::new(0));
        let completion = Arc::new(Semaphore::new(0));

        let diagnostics = Arc::new(DispatchDiagnostics::default());
        let dispatcher = {
            let started = Arc::clone(&started);
            let release_requests = Arc::clone(&release_requests);
            let release_rejection = Arc::clone(&release_rejection);
            let completion = Arc::clone(&completion);
            crate::rt::spawn(run_event_dispatcher(
                rx,
                edge::testing::edge(),
                Arc::clone(&diagnostics),
                move |event, admission| {
                    let started = Arc::clone(&started);
                    let release_requests = Arc::clone(&release_requests);
                    let release_rejection = Arc::clone(&release_rejection);
                    let completion = Arc::clone(&completion);
                    async move {
                        match (event, admission) {
                            (TestEvent::Request(index), DispatchAdmission::Admitted)
                                if index < MAX_ACTIVE_REQUESTS =>
                            {
                                started.add_permits(1);
                                let _permit = release_requests.acquire().await.unwrap();
                            }
                            (TestEvent::Request(_), DispatchAdmission::Saturated) => {
                                let _permit = release_rejection.acquire().await.unwrap();
                            }
                            (TestEvent::Completion, DispatchAdmission::Admitted) => {
                                completion.add_permits(1);
                            }
                            _ => {}
                        }
                    }
                },
            ))
        };

        for index in 0..MAX_ACTIVE_REQUESTS {
            tx.send(TestEvent::Request(index)).await.unwrap();
        }
        started
            .acquire_many(MAX_ACTIVE_REQUESTS as u32)
            .await
            .unwrap()
            .forget();
        for index in MAX_ACTIVE_REQUESTS..MAX_ACTIVE_REQUESTS + MAX_ACTIVE_REJECTIONS + 1 {
            tx.send(TestEvent::Request(index)).await.unwrap();
        }
        tx.send(TestEvent::Completion).await.unwrap();

        crate::rt::time::timeout(Duration::from_secs(1), completion.acquire())
            .await
            .expect("a stalled nack must not park the transport drain")
            .unwrap()
            .forget();
        let saturated = diagnostics.snapshot();
        assert_eq!(
            saturated.active_rejections, MAX_ACTIVE_REJECTIONS,
            "stalled capacity replies must consume only the fixed rejection slots"
        );
        assert_eq!(saturated.rejection_dropped_total, 1);

        release_rejection.add_permits(MAX_ACTIVE_REJECTIONS);
        release_requests.add_permits(MAX_ACTIVE_REQUESTS);
        crate::rt::time::timeout(Duration::from_secs(1), async {
            loop {
                let snapshot = diagnostics.snapshot();
                if snapshot.active_requests == 0 && snapshot.active_rejections == 0 {
                    break;
                }
                crate::rt::yield_now().await;
            }
        })
        .await
        .expect("completed dispatcher handles must be reaped while the event channel stays open");
        drop(tx);
        dispatcher.join().await.unwrap();
    }

    #[nuclei::test(threads = 0)]
    async fn stalled_completion_does_not_block_recovery_serving() {
        let (tx, rx) = mpsc::channel(8);
        let completion_started = Arc::new(Semaphore::new(0));
        let release_completion = Arc::new(Semaphore::new(0));
        let request_observed = Arc::new(Semaphore::new(0));
        let diagnostics = Arc::new(DispatchDiagnostics::default());

        let dispatcher = {
            let completion_started = Arc::clone(&completion_started);
            let release_completion = Arc::clone(&release_completion);
            let request_observed = Arc::clone(&request_observed);
            crate::rt::spawn(run_event_dispatcher(
                rx,
                edge::testing::edge(),
                Arc::clone(&diagnostics),
                move |event, admission| {
                    let completion_started = Arc::clone(&completion_started);
                    let release_completion = Arc::clone(&release_completion);
                    let request_observed = Arc::clone(&request_observed);
                    async move {
                        match (event, admission) {
                            (TestEvent::Completion, DispatchAdmission::Admitted) => {
                                completion_started.add_permits(1);
                                let _permit = release_completion.acquire().await.unwrap();
                            }
                            (TestEvent::Recovery(_), DispatchAdmission::Admitted) => {
                                request_observed.add_permits(1);
                            }
                            _ => {}
                        }
                    }
                },
            ))
        };

        tx.send(TestEvent::Completion).await.unwrap();
        completion_started.acquire().await.unwrap().forget();
        tx.send(TestEvent::Recovery(0)).await.unwrap();

        crate::rt::time::timeout(Duration::from_secs(1), request_observed.acquire())
            .await
            .expect("durable completion work must not head-of-line block CAR serving")
            .unwrap()
            .forget();
        assert_eq!(diagnostics.snapshot().active_completions, 1);

        release_completion.add_permits(1);
        drop(tx);
        dispatcher.join().await.unwrap();
        assert_eq!(diagnostics.snapshot().active_completions, 0);
    }

    #[nuclei::test(threads = 0)]
    async fn full_completion_workers_do_not_block_requests_on_a_per_class_queue() {
        let (tx, rx) = crate::sync::dispatch_queue(64);
        let completions_started = Arc::new(regolith::sync::Semaphore::new(0));
        let release_completions = Arc::new(regolith::sync::Semaphore::new(0));
        let request_observed = Arc::new(regolith::sync::Semaphore::new(0));

        let dispatcher = {
            let completions_started = Arc::clone(&completions_started);
            let release_completions = Arc::clone(&release_completions);
            let request_observed = Arc::clone(&request_observed);
            crate::rt::spawn(run_event_dispatcher(
                rx,
                edge::testing::edge(),
                Arc::new(DispatchDiagnostics::default()),
                move |event, _admission| {
                    let completions_started = Arc::clone(&completions_started);
                    let release_completions = Arc::clone(&release_completions);
                    let request_observed = Arc::clone(&request_observed);
                    async move {
                        match event {
                            TestEvent::Completion => {
                                completions_started.add_permits(1);
                                let _permit = release_completions.acquire(1).await;
                            }
                            TestEvent::Request(_) => request_observed.add_permits(1),
                            TestEvent::Recovery(_) | TestEvent::Gossip(_) => {}
                        }
                    }
                },
            ))
        };

        for _ in 0..MAX_ACTIVE_COMPLETIONS {
            tx.send(TestEvent::Completion).await.unwrap();
        }
        completions_started
            .acquire(MAX_ACTIVE_COMPLETIONS)
            .await
            .forget();
        tx.send(TestEvent::Completion).await.unwrap();
        tx.send(TestEvent::Request(0)).await.unwrap();

        crate::rt::time::timeout(Duration::from_secs(1), request_observed.acquire(1))
            .await
            .expect("busy completion workers must not hold up a request")
            .forget();

        release_completions.add_permits(MAX_ACTIVE_COMPLETIONS + 1);
        drop(tx);
        dispatcher.join().await.unwrap();
    }

    #[nuclei::test(threads = 0)]
    async fn a_lease_is_held_until_its_handler_returns() {
        struct Flag(Arc<std::sync::atomic::AtomicBool>);
        impl Drop for Flag {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        let (tx, rx) = crate::sync::dispatch_queue(4);
        let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let released_during_handler = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let handled = Arc::new(regolith::sync::Semaphore::new(0));
        let dispatcher = {
            let handled = Arc::clone(&handled);
            let released = Arc::clone(&released);
            let released_during_handler = Arc::clone(&released_during_handler);
            crate::rt::spawn(run_event_dispatcher(
                rx,
                edge::testing::edge(),
                Arc::new(DispatchDiagnostics::default()),
                move |_event: TestEvent, _admission| {
                    let released = Arc::clone(&released);
                    let released_during_handler = Arc::clone(&released_during_handler);
                    let handled = Arc::clone(&handled);
                    async move {
                        crate::rt::yield_now().await;
                        released_during_handler
                            .store(released.load(Ordering::SeqCst), Ordering::SeqCst);
                        handled.add_permits(1);
                    }
                },
            ))
        };

        tx.send_leased(TestEvent::Request(0), Box::new(Flag(Arc::clone(&released))))
            .await
            .unwrap();
        handled.acquire(1).await.forget();
        drop(tx);
        dispatcher.join().await.unwrap();

        assert!(!released_during_handler.load(Ordering::SeqCst));
        assert!(released.load(Ordering::SeqCst));
    }
}
