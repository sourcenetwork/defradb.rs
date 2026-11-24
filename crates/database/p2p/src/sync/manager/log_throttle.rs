//! Lock-free gates that let one log line through per window and count the
//! rest. Exact event counts stay in [`super::SyncDiagnostics`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::rt::time::Instant;
use kovan_map::HopscotchMap;

pub(crate) const LOG_THROTTLE_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogAdmission {
    /// Log at full level; `suppressed` occurrences were demoted since the last.
    Full {
        suppressed: u64,
    },
    Suppressed,
}

#[derive(Debug)]
pub(crate) struct LogThrottle {
    base: Instant,
    next_full_ms: AtomicU64,
    suppressed: AtomicU64,
}

impl Default for LogThrottle {
    fn default() -> Self {
        Self {
            base: Instant::now(),
            next_full_ms: AtomicU64::new(0),
            suppressed: AtomicU64::new(0),
        }
    }
}

impl LogThrottle {
    pub(crate) fn admit(&self) -> LogAdmission {
        let now_ms = self.base.elapsed().as_millis() as u64;
        let next_full_ms = self.next_full_ms.load(Ordering::Relaxed);
        if now_ms >= next_full_ms
            && self
                .next_full_ms
                .compare_exchange(
                    next_full_ms,
                    now_ms.saturating_add(LOG_THROTTLE_WINDOW.as_millis() as u64),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
        {
            return LogAdmission::Full {
                suppressed: self.suppressed.swap(0, Ordering::Relaxed),
            };
        }
        self.suppressed.fetch_add(1, Ordering::Relaxed);
        LogAdmission::Suppressed
    }
}

/// One slot per `WindowFailure`, plus one for a line with no recorded failure.
pub(crate) const DAG_FETCH_LOG_SLOTS: usize = 9;

/// Roots exhausted against one provider set and not yet summarized.
#[derive(Debug, Default)]
pub(crate) struct ExhaustionTally {
    /// Counted before `roots`, so a summary that drains `roots` never sees
    /// more roots than `pending` holds.
    pub(crate) pending: AtomicU64,
    pub(crate) roots: [AtomicU64; DAG_FETCH_LOG_SLOTS],
    /// Roots whose own exhaustion line was admitted at full level.
    pub(crate) logged_full: AtomicU64,
}

pub(crate) type ExhaustionTallies =
    Arc<HopscotchMap<String, Arc<ExhaustionTally>, rapidhash::fast::RandomState>>;

/// Throttled per line and per dominant failure kind, so a rare kind still
/// gets its own full line while another floods.
#[derive(Debug)]
pub(crate) struct DagFetchLogThrottles {
    pub(crate) no_progress_batch: [LogThrottle; DAG_FETCH_LOG_SLOTS],
    pub(crate) retry: [LogThrottle; DAG_FETCH_LOG_SLOTS],
    pub(crate) exhausted: [LogThrottle; DAG_FETCH_LOG_SLOTS],
    pub(crate) exhausted_by_providers: ExhaustionTallies,
    /// Where the summary tasks run; set once by the coordinator that owns
    /// these throttles. Until it is set, exhaustions are counted but no
    /// summary line is scheduled.
    network: regolith::sync::OnceCell<crate::network::Network>,
}

impl DagFetchLogThrottles {
    /// Runs the summary tasks on `network`. The first call wins.
    pub(crate) fn use_network(&self, network: crate::network::Network) {
        let _ = self.network.set(network);
    }

    pub(crate) fn network(&self) -> Option<&crate::network::Network> {
        self.network.get()
    }
}

impl Default for DagFetchLogThrottles {
    fn default() -> Self {
        Self {
            no_progress_batch: Default::default(),
            retry: Default::default(),
            exhausted: Default::default(),
            exhausted_by_providers: Arc::new(HopscotchMap::with_hasher(Default::default())),
            network: regolith::sync::OnceCell::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[nuclei::test(start_paused = true)]
    async fn one_full_line_per_window_reports_the_suppressed_count() {
        let throttle = LogThrottle::default();
        assert_eq!(throttle.admit(), LogAdmission::Full { suppressed: 0 });
        for _ in 0..9 {
            assert_eq!(throttle.admit(), LogAdmission::Suppressed);
        }

        nuclei::time::advance(LOG_THROTTLE_WINDOW - Duration::from_secs(1)).await;
        assert_eq!(throttle.admit(), LogAdmission::Suppressed);

        nuclei::time::advance(Duration::from_secs(1)).await;
        assert_eq!(throttle.admit(), LogAdmission::Full { suppressed: 10 });
        assert_eq!(throttle.admit(), LogAdmission::Suppressed);
    }
}
