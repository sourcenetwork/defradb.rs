use kovan::Atom;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use web_time::Instant;

use p2p::message::PushLogReply;
use p2p::transport::PeerId;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Maximum concurrent per-document push tasks during initial replay.
///
/// Lower than the coordinator's live push limit (32) because initial replay
/// is background work that shouldn't starve real-time sync traffic.
pub(crate) const MAX_CONCURRENT_REPLAY_TASKS: usize = 8;

/// Maximum concurrent outbound PushLog requests across replay tasks.
pub const DEFAULT_MAX_CONCURRENT_REPLAY_SENDS: usize = 8;

/// Default per-peer replay burst. Mirrors the sync manager's live rate limiter.
pub const DEFAULT_REPLAY_RATE_LIMIT_BURST: u32 = p2p::sync::DEFAULT_RATE_LIMIT_BURST;

/// Default per-peer replay refill rate. Mirrors the sync manager's live rate limiter.
pub const DEFAULT_REPLAY_RATE_LIMIT_RATE: f64 = p2p::sync::DEFAULT_RATE_LIMIT_RATE;

/// Default timeout for a single replay PushLog request.
pub const DEFAULT_REPLAY_SEND_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct ReplayPushConfig {
    /// Retry schedule shared with live replication after replay fails.
    pub retry_schedule: storage::stores::RetrySchedule,

    /// Maximum number of documents whose DAG blocks may be replayed concurrently.
    pub max_concurrent_document_tasks: usize,

    /// Maximum number of outbound PushLog requests in flight across all replay tasks.
    pub max_concurrent_outbound_pushes: usize,

    /// Number of per-peer replay tokens available for short bursts.
    pub per_peer_rate_limit_burst: u32,

    /// Number of per-peer replay tokens refilled per second.
    pub per_peer_rate_limit_rate: f64,

    /// Timeout for one outbound replay PushLog request.
    pub send_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayDocumentFailure {
    pub doc_id: String,
    pub collection_id: String,
}

/// Persist the typed hint before crossing the legacy string error boundary.
///
/// The first history replay has no retry-info row yet — only the replicator
/// exists — so a plain reschedule writes nothing and reports `Ok(false)`;
/// the initial schedule is seeded with the hint's deadline instead. A
/// replicator that is gone stays a success: no durable obligation to defer.
pub async fn persist_retry_after<S: storage::corekv::Store>(
    peerstore: &storage::stores::Peerstore<S>,
    peer_id: &PeerId,
    reply: &PushLogReply,
) -> Result<(), String> {
    let Some(delay) = reply.retry_after() else {
        return Ok(());
    };
    match peerstore
        .reschedule_retry_peer(peer_id.as_str(), Some(delay), 0)
        .await
    {
        Ok(true) => Ok(()),
        // A missing schedule row is the first history replay; seed it with the
        // hint's deadline. A missing replicator stays a success: no durable
        // obligation to defer.
        Ok(false) => peerstore
            .seed_retry_peer(peer_id.as_str(), delay)
            .await
            .map(|_| ())
            .map_err(|error| format!("failed to persist receiver retry-after: {error}")),
        Err(error) => Err(format!("failed to persist receiver retry-after: {error}")),
    }
}

pub async fn remaining_retry_after<S: storage::corekv::Store>(
    peerstore: &storage::stores::Peerstore<S>,
    peer_id: &PeerId,
) -> Result<Option<Duration>, String> {
    let Some(bytes) = peerstore
        .get_retry_info(peer_id.as_str())
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let info = storage::stores::RetryInfo::from_bytes(&bytes)?;
    let now = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap_or_default();
    Ok(Duration::from_secs(info.not_before_unix)
        .checked_sub(now)
        .filter(|delay| !delay.is_zero()))
}

pub async fn check_retry_admission<S: storage::corekv::Store>(
    peerstore: &storage::stores::Peerstore<S>,
    peer_id: &PeerId,
) -> Result<(), ReplayPushSendError> {
    match remaining_retry_after(peerstore, peer_id)
        .await
        .map_err(ReplayPushSendError::Persistence)?
    {
        Some(retry_after) => Err(ReplayPushSendError::Backpressure { retry_after }),
        None => Ok(()),
    }
}

/// Persist documents that did not finish their initial replay so the normal
/// retry sweep owns them after the bounded attempt. Replay intentionally does
/// not hold the peer writer across network waits, so the failure handoff must
/// reacquire it before mutating the shared peer schedule.
pub async fn persist_replay_failures<S: storage::corekv::Store>(
    peerstore: &storage::stores::Peerstore<S>,
    peer_id: &PeerId,
    failures: &[ReplayDocumentFailure],
) -> Result<(), String> {
    if failures.is_empty() {
        return Ok(());
    }

    let Some(_retry_guard) = peerstore
        .acquire_replicator_retry_guard(peer_id.as_str())
        .await
        .map_err(|error| format!("failed to coordinate replay failure persistence: {error}"))?
    else {
        // Forget won the race with the completed network attempt.  The removed
        // replicator no longer owns a durable delivery obligation.
        return Ok(());
    };
    for failure in failures {
        let retry_info = storage::stores::RetryInfo::new_initial()
            .to_bytes()
            .map_err(|error| format!("failed to serialize replay retry state: {error}"))?;
        // Initial replay resolves the current document heads again on retry,
        // so this is deliberately a scope marker without a payload CID.
        peerstore
            .record_push_failure(
                peer_id.as_str(),
                &failure.doc_id,
                &failure.collection_id,
                &retry_info,
            )
            .await
            .map_err(|error| {
                format!(
                    "failed to persist replay retry for document {}: {error}",
                    failure.doc_id
                )
            })?;
    }

    // Match the live-push failure recorder's observable state. Retry records
    // remain authoritative even if legacy/invalid replicator bytes prevent the
    // status update.
    if let Some(bytes) = peerstore
        .get_replicator(peer_id.as_str())
        .await
        .map_err(|error| format!("failed to load persisted replicator status: {error}"))?
    {
        match p2p::ReplicatorInfo::from_bytes(&bytes) {
            Ok(mut info) => {
                if info.set_status_if_changed_now(p2p::ReplicatorStatus::Inactive) {
                    let bytes = info.to_bytes().map_err(|error| {
                        format!("failed to encode inactive replicator status: {error}")
                    })?;
                    peerstore
                        .create_replicator(peer_id.as_str(), &bytes)
                        .await
                        .map_err(|error| {
                            format!("failed to persist inactive replicator status: {error}")
                        })?;
                }
            }
            Err(error) => {
                tracing::warn!(
                    peer_id = %peer_id,
                    %error,
                    "Replay retries were recorded but replicator status could not be decoded"
                );
            }
        }
    }

    tracing::warn!(
        peer_id = %peer_id,
        failure_count = failures.len(),
        "Initial replay left unfinished documents; deferred them to persisted retry"
    );
    Ok(())
}

impl Default for ReplayPushConfig {
    fn default() -> Self {
        Self {
            retry_schedule: storage::stores::RetrySchedule::default(),
            max_concurrent_document_tasks: MAX_CONCURRENT_REPLAY_TASKS,
            max_concurrent_outbound_pushes: DEFAULT_MAX_CONCURRENT_REPLAY_SENDS,
            per_peer_rate_limit_burst: DEFAULT_REPLAY_RATE_LIMIT_BURST,
            per_peer_rate_limit_rate: DEFAULT_REPLAY_RATE_LIMIT_RATE,
            send_timeout: DEFAULT_REPLAY_SEND_TIMEOUT,
        }
    }
}

#[derive(Debug)]
pub enum ReplayPushSendError {
    Backpressure { retry_after: Duration },
    Persistence(String),
    SemaphoreClosed,
    Timeout { timeout: Duration },
    Transport(p2p::Error),
}

impl ReplayPushSendError {
    pub(crate) fn is_connection_like(&self) -> bool {
        matches!(self, Self::Transport(error) if error.is_connection_like())
    }
}

impl fmt::Display for ReplayPushSendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Persistence(error) => write!(f, "replay admission state: {error}"),
            Self::Backpressure { .. } => {
                f.write_str("receiver backpressure; deferred to persisted retry")
            }
            Self::SemaphoreClosed => f.write_str("replay send semaphore closed"),
            Self::Timeout { timeout } => {
                write!(f, "replay PushLog timed out after {}s", timeout.as_secs())
            }
            Self::Transport(error) => write!(f, "{error}"),
        }
    }
}

pub struct ReplayPushGate {
    document_task_semaphore: Arc<Semaphore>,
    outbound_push_semaphore: Arc<Semaphore>,
    peer_pacer: ReplayPeerPacer,
    send_timeout: Duration,
}

impl ReplayPushGate {
    pub fn new(config: ReplayPushConfig) -> Self {
        let rate = if config.per_peer_rate_limit_rate.is_finite()
            && config.per_peer_rate_limit_rate > 0.0
        {
            config.per_peer_rate_limit_rate
        } else {
            DEFAULT_REPLAY_RATE_LIMIT_RATE
        };

        Self {
            document_task_semaphore: Arc::new(Semaphore::new(
                config.max_concurrent_document_tasks.max(1),
            )),
            outbound_push_semaphore: Arc::new(Semaphore::new(
                config.max_concurrent_outbound_pushes.max(1),
            )),
            peer_pacer: ReplayPeerPacer::new(config.per_peer_rate_limit_burst.max(1), rate),
            send_timeout: config.send_timeout.max(Duration::from_millis(1)),
        }
    }

    pub(crate) async fn acquire_document_task(
        &self,
    ) -> Result<OwnedSemaphorePermit, ReplayPushSendError> {
        self.document_task_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ReplayPushSendError::SemaphoreClosed)
    }

    pub async fn send_pushlog<F>(
        &self,
        peer_id: &PeerId,
        send: F,
    ) -> Result<PushLogReply, ReplayPushSendError>
    where
        F: Future<Output = p2p::Result<PushLogReply>>,
    {
        self.send_pushlog_with_admission(peer_id, async { Ok(()) }, send)
            .await
    }

    /// Check durable admission after pacing and acquiring a send slot, so a
    /// hint installed while waiting cannot be bypassed by a stale preflight.
    pub async fn send_pushlog_with_admission<F, A>(
        &self,
        peer_id: &PeerId,
        admission: A,
        send: F,
    ) -> Result<PushLogReply, ReplayPushSendError>
    where
        F: Future<Output = p2p::Result<PushLogReply>>,
        A: Future<Output = Result<(), ReplayPushSendError>>,
    {
        while let Some(delay) = self.peer_pacer.consume_or_delay(peer_id.as_str()) {
            if let Some(retry_after) = self.peer_pacer.retry_after(peer_id.as_str()) {
                return Err(ReplayPushSendError::Backpressure { retry_after });
            }
            n0_future::time::sleep(delay).await;
        }

        let _permit = self
            .outbound_push_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ReplayPushSendError::SemaphoreClosed)?;

        if let Some(retry_after) = self.peer_pacer.retry_after(peer_id.as_str()) {
            return Err(ReplayPushSendError::Backpressure { retry_after });
        }
        admission.await?;
        let reply = n0_future::time::timeout(self.send_timeout, send)
            .await
            .map_err(|_| ReplayPushSendError::Timeout {
                timeout: self.send_timeout,
            })?
            .map_err(ReplayPushSendError::Transport)?;
        if let Some(delay) = reply.retry_after() {
            let bucket = self.peer_pacer.bucket(peer_id.as_str());
            loop {
                let current = bucket.load();
                let mut next = *current;
                next.blocked_until = Some(
                    next.blocked_until
                        .unwrap_or_else(Instant::now)
                        .max(Instant::now() + delay),
                );
                if bucket.compare_and_swap(&current, next).is_ok() {
                    break;
                }
            }
        }
        Ok(reply)
    }
}

struct ReplayPeerPacer {
    buckets: HopscotchMap<String, Arc<Atom<ReplayPeerBucket>>, RandomState>,
    capacity: u32,
    refill_rate: f64,
}

impl ReplayPeerPacer {
    fn retry_after(&self, peer_id: &str) -> Option<Duration> {
        self.bucket(peer_id)
            .load()
            .blocked_until
            .and_then(|until| until.checked_duration_since(Instant::now()))
            .filter(|delay| !delay.is_zero())
    }

    fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            buckets: HopscotchMap::with_hasher(RandomState::default()),
            capacity,
            refill_rate,
        }
    }

    fn bucket(&self, peer_id: &str) -> Arc<Atom<ReplayPeerBucket>> {
        if let Some(bucket) = self.buckets.get(peer_id) {
            return bucket;
        }
        self.buckets.get_or_insert(
            peer_id.to_string(),
            Arc::new(Atom::new(ReplayPeerBucket::new(self.capacity))),
        )
    }

    fn consume_or_delay(&self, peer_id: &str) -> Option<Duration> {
        let bucket = self.bucket(peer_id);
        loop {
            let current = bucket.load();
            let mut next = *current;
            let delay = next.consume_or_delay(self.capacity, self.refill_rate);
            if bucket.compare_and_swap(&current, next).is_ok() {
                return delay;
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ReplayPeerBucket {
    tokens: f64,
    last_refill: Instant,
    blocked_until: Option<Instant>,
}

impl ReplayPeerBucket {
    fn new(capacity: u32) -> Self {
        Self {
            tokens: capacity as f64,
            last_refill: Instant::now(),
            blocked_until: None,
        }
    }

    fn consume_or_delay(&mut self, capacity: u32, refill_rate: f64) -> Option<Duration> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * refill_rate).min(capacity as f64);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            None
        } else {
            let refill_delay = (1.0 - self.tokens) / refill_rate;
            Some(Duration::from_secs_f64(refill_delay.clamp(0.001, 1.0)))
        }
    }
}
