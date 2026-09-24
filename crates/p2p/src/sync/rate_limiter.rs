//! Per-peer token-bucket rate limiter for P2P event dispatch.
//!
//! Limits the rate at which any single peer can drive expensive coordinator
//! operations (DocSync requests, PushLog broadcasts, etc.).  Each peer starts
//! with a full bucket of tokens; one token is consumed per allowed event.
//! Tokens refill at a constant rate up to the bucket capacity.

use kovan::Atom;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use std::sync::Arc;
use std::time::Duration;
use web_time::Instant;

use crate::transport::PeerId;

use super::manager::{
    default_rate_limit_backoff, DEFAULT_RATE_LIMIT_BURST, DEFAULT_RATE_LIMIT_RATE,
};

/// Maximum number of peer buckets to retain.
///
/// Disconnected peers that have not generated traffic for a while are evicted
/// lazily on the next insertion when this limit is hit.
const MAX_TRACKED_PEERS: usize = 10_000;

/// The token-bucket state of one peer; replaced as a unit on every check.
#[derive(Debug, Clone, Copy)]
struct BucketState {
    /// Current token count (may be fractional internally, stored as f64).
    tokens: f64,
    /// When tokens were last refilled.
    last_refill: Instant,
    /// Consecutive rate-limit refusals since the last allowed event.
    consecutive_failures: u32,
    /// Earliest time this peer should be retried after a refusal.
    next_retry_after: Option<Instant>,
}

impl BucketState {
    fn new(capacity: u32) -> Self {
        Self {
            tokens: capacity as f64,
            last_refill: Instant::now(),
            consecutive_failures: 0,
            next_retry_after: None,
        }
    }

    /// Refill tokens based on elapsed time and return the successor state
    /// with whether one token was available to consume.
    fn consume(
        mut self,
        now: Instant,
        capacity: u32,
        refill_rate: f64,
        backoff_steps: &[Duration],
    ) -> (Self, RateLimitDecision) {
        if let Some(next_retry_after) = self.next_retry_after {
            if next_retry_after > now {
                return (
                    self,
                    RateLimitDecision::Limited {
                        retry_after: next_retry_after.duration_since(now),
                        consecutive_failures: self.consecutive_failures,
                    },
                );
            }
            self.next_retry_after = None;
        }

        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * refill_rate).min(capacity as f64);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            self.consecutive_failures = 0;
            (self, RateLimitDecision::Allowed)
        } else {
            self.consecutive_failures = self.consecutive_failures.saturating_add(1);
            let retry_after = backoff_for_failure(backoff_steps, self.consecutive_failures);
            self.next_retry_after = Some(now + retry_after);
            let consecutive_failures = self.consecutive_failures;
            (
                self,
                RateLimitDecision::Limited {
                    retry_after,
                    consecutive_failures,
                },
            )
        }
    }
}

#[derive(Debug)]
struct Bucket {
    peer: String,
    state: Atom<BucketState>,
}

fn backoff_for_failure(backoff_steps: &[Duration], consecutive_failures: u32) -> Duration {
    let index = consecutive_failures.saturating_sub(1) as usize;
    backoff_steps
        .get(index)
        .or_else(|| backoff_steps.last())
        .copied()
        .unwrap_or_else(|| Duration::from_secs(1))
}

/// Minimum effective refill rate for the request-intake limiter (tokens/s).
///
/// One token per second bounds receiver admission latency independently of the
/// sender's durable 30-second marker ladder.
pub const MIN_REQUEST_REFILL_RATE: f64 = 1.0;

/// One-token refill horizon for request-intake pacing, clamped to [5ms, 1s].
fn request_pacing_backoff(refill_rate: f64) -> Vec<Duration> {
    let rate = if refill_rate.is_finite() && refill_rate > 0.0 {
        refill_rate
    } else {
        1.0
    };
    vec![Duration::from_secs_f64((1.0 / rate).clamp(0.005, 1.0))]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RateLimitDecision {
    Allowed,
    Limited {
        retry_after: Duration,
        consecutive_failures: u32,
    },
}

/// Per-peer rate limiter backed by token buckets.
///
/// Lock-free: each peer's bucket is replaced atomically as one unit, so
/// concurrent checks for the same peer never consume the same token twice.
/// Designed to be held behind an `Arc` and shared across event-handler
/// invocations.
///
/// Uses string-based peer IDs to support both libp2p and iroh transports.
pub struct PeerRateLimiter {
    buckets: HopscotchMap<String, Arc<Bucket>, RandomState>,
    capacity: u32,
    refill_rate: f64,
    backoff_steps: Vec<Duration>,
}

impl Default for PeerRateLimiter {
    fn default() -> Self {
        Self::new(DEFAULT_RATE_LIMIT_BURST, DEFAULT_RATE_LIMIT_RATE)
    }
}

impl PeerRateLimiter {
    /// Create a new limiter with the given capacity and refill rate.
    ///
    /// * `capacity`: maximum tokens per peer (burst size).
    /// * `refill_rate`: tokens added per second per peer.
    pub fn new(capacity: u32, refill_rate: f64) -> Self {
        Self::with_backoff_steps(capacity, refill_rate, default_rate_limit_backoff())
    }

    /// Create a request-intake limiter: same bucket parameters, but the retry
    /// horizon after a refusal is ~one token refill instead of the abuse
    /// ladder.
    ///
    /// Request paths (PushLog, TwoStream, DocSync, ...) have a reply channel
    /// and a well-behaved durable retry protocol, so the bucket itself is the
    /// receiver-side flow control. A long local lockout would reject later
    /// attempts even after the receiver had drained.
    /// Gossip keeps the
    /// abuse ladder (drop-only, no reply channel).
    ///
    /// The effective refill rate is floored at
    /// [`MIN_REQUEST_REFILL_RATE`] so receiver admission becomes available
    /// promptly once work drains. Sender timing remains owned by the durable
    /// marker ladder.
    pub fn new_request_paced(capacity: u32, refill_rate: f64) -> Self {
        let rate = if refill_rate.is_finite() && refill_rate > MIN_REQUEST_REFILL_RATE {
            refill_rate
        } else {
            MIN_REQUEST_REFILL_RATE
        };
        Self::with_backoff_steps(capacity, rate, request_pacing_backoff(rate))
    }

    /// Create a new limiter with explicit rate-limit backoff steps.
    pub fn with_backoff_steps(
        capacity: u32,
        refill_rate: f64,
        backoff_steps: Vec<Duration>,
    ) -> Self {
        Self {
            buckets: HopscotchMap::with_hasher(RandomState::default()),
            capacity,
            refill_rate,
            backoff_steps,
        }
    }

    /// Attempt to consume one token for `peer`.
    ///
    /// Returns backoff metadata when the peer is rate-limited.
    pub(crate) fn check(&self, peer: &PeerId) -> RateLimitDecision {
        let bucket = self
            .buckets
            .get(peer.as_str())
            .unwrap_or_else(|| self.admit(peer.as_str()));
        loop {
            let current = bucket.state.load();
            let (next, decision) = (*current).consume(
                Instant::now(),
                self.capacity,
                self.refill_rate,
                &self.backoff_steps,
            );
            if bucket.state.compare_and_swap(&current, next).is_ok() {
                return decision;
            }
        }
    }

    fn admit(&self, peer: &str) -> Arc<Bucket> {
        if self.buckets.len() >= MAX_TRACKED_PEERS {
            let oldest = self
                .buckets
                .values()
                .min_by_key(|bucket| bucket.state.peek(|state| state.last_refill));
            if let Some(oldest) = oldest {
                self.buckets.remove(&oldest.peer);
            }
        }
        self.buckets.get_or_insert(
            peer.to_string(),
            Arc::new(Bucket {
                peer: peer.to_string(),
                state: Atom::new(BucketState::new(self.capacity)),
            }),
        )
    }

    /// Discard the bucket for `peer` (called on disconnect to free memory).
    pub fn remove_peer(&self, peer: &PeerId) {
        self.buckets.remove(peer.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limited(decision: RateLimitDecision) -> (Duration, u32) {
        match decision {
            RateLimitDecision::Allowed => panic!("expected rate-limited decision"),
            RateLimitDecision::Limited {
                retry_after,
                consecutive_failures,
            } => (retry_after, consecutive_failures),
        }
    }

    #[test]
    fn peer_backoff_grows_after_repeated_rate_limit_windows() {
        let limiter = PeerRateLimiter::with_backoff_steps(
            0,
            0.0,
            vec![
                Duration::from_millis(1),
                Duration::from_millis(2),
                Duration::from_millis(4),
            ],
        );
        let peer = PeerId::new("peer-1".to_string());

        let (_, failures) = limited(limiter.check(&peer));
        assert_eq!(failures, 1);

        std::thread::sleep(Duration::from_millis(2));
        let (_, failures) = limited(limiter.check(&peer));
        assert_eq!(failures, 2);

        std::thread::sleep(Duration::from_millis(3));
        let (_, failures) = limited(limiter.check(&peer));
        assert_eq!(failures, 3);
    }

    #[test]
    fn request_paced_limiter_recovers_at_refill_horizon_not_ladder() {
        // #1088 W4 follow-up: request-intake limiting is flow control, not
        // abuse control. Receiver admission must reopen at the refill horizon;
        // the sender independently retains its durable retry marker.
        let limiter = PeerRateLimiter::new_request_paced(1, 200.0);
        let peer = PeerId::new("peer-1".to_string());

        assert_eq!(limiter.check(&peer), RateLimitDecision::Allowed);
        let (retry_after, _) = limited(limiter.check(&peer));
        assert!(
            retry_after <= Duration::from_millis(50),
            "retry horizon must be ~one token refill, not the abuse ladder: {retry_after:?}"
        );

        std::thread::sleep(retry_after + Duration::from_millis(20));
        assert_eq!(
            limiter.check(&peer),
            RateLimitDecision::Allowed,
            "must recover as soon as a token refills"
        );
    }

    #[test]
    fn request_paced_limiter_floors_pathological_refill_rates() {
        // A configured rate of 0.1 tokens/s refills one token per 10s. Floor
        // pathological values so receiver admission reopens promptly; this
        // does not alter the sender's durable retry schedule.
        let limiter = PeerRateLimiter::new_request_paced(1, 0.1);
        let peer = PeerId::new("peer-1".to_string());

        assert_eq!(limiter.check(&peer), RateLimitDecision::Allowed);
        let (retry_after, _) = limited(limiter.check(&peer));
        assert!(
            retry_after <= Duration::from_secs(1),
            "retry horizon must stay within the pusher's in-batch budget: {retry_after:?}"
        );

        std::thread::sleep(retry_after + Duration::from_millis(120));
        assert_eq!(
            limiter.check(&peer),
            RateLimitDecision::Allowed,
            "a token must actually refill within the advertised horizon, not at the raw 0.1/s rate"
        );
    }

    #[test]
    fn peer_backoff_resets_after_allowed_request() {
        let limiter = PeerRateLimiter::with_backoff_steps(
            1,
            1_000.0,
            vec![Duration::from_millis(1), Duration::from_millis(2)],
        );
        let peer = PeerId::new("peer-1".to_string());

        assert_eq!(limiter.check(&peer), RateLimitDecision::Allowed);
        let (_, failures) = limited(limiter.check(&peer));
        assert_eq!(failures, 1);

        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(limiter.check(&peer), RateLimitDecision::Allowed);
        let (_, failures) = limited(limiter.check(&peer));
        assert_eq!(failures, 1);
    }
}
