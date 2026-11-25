//! The stuck-poll monitor. Each worker publishes which poll it is in: a task
//! poll with when it began, or a hook poll, which the scan times from when
//! it first sees it (so a slow hook is found up to one scan period late,
//! and the worker reads no clock for it). A scan on a timer finds a poll
//! past the limit, reports it once per task or once per hook poll (its
//! name, its worker, how long), marks the worker stuck so routing sends
//! it nothing new, and moves the movable jobs still queued in its inbox to
//! free workers on the same node. Bound jobs wait for their owner, and no
//! running task is ever moved.
//!
//! The timer is a [`MonitorDriver`]'s when the configuration gives one (the
//! network edge's, so no worker spends a poll watching the others);
//! otherwise two workers run it, so one of them always watches the other.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::Duration;

use super::PoolShared;
use crate::worker::{micros, report_long_hook, report_long_poll, REPORTED};

/// How many workers run the scan when no driver does.
pub(crate) const WATCHERS: usize = 2;

/// Runs the stuck-poll scan on a timer outside the pool: called once when
/// the pool starts, with the [`Monitor`] to run every
/// [`Monitor::period`] until [`Monitor::scan`] returns `None`.
pub type MonitorDriver = Arc<dyn Fn(Monitor) + Send + Sync>;

/// The pool's stuck-poll scan, for a [`MonitorDriver`] to run. Holds the pool
/// weakly, so a driver never keeps a pool alive.
#[derive(Clone)]
pub struct Monitor {
    pool: Weak<PoolShared>,
    period: Duration,
}

impl Monitor {
    pub(crate) fn of(pool: &Arc<PoolShared>) -> Monitor {
        Monitor {
            pool: Arc::downgrade(pool),
            period: period(pool.stuck_after),
        }
    }

    /// How often to scan.
    pub fn period(&self) -> Duration {
        self.period
    }

    /// Scans every worker now: how many are stuck, or `None` once the pool
    /// is gone or every worker has stopped, when the driver stops.
    pub fn scan(&self) -> Option<usize> {
        let pool = self.pool.upgrade()?;
        if pool.stopped.is_set() {
            return None;
        }
        Some(scan(&pool, pool.clock.now_micros()))
    }
}

impl std::fmt::Debug for Monitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Monitor")
            .field("period", &self.period)
            .finish_non_exhaustive()
    }
}

/// How often a watcher scans, for a stuck limit of `limit`.
pub(crate) fn period(limit: Duration) -> Duration {
    (limit / 2).max(Duration::from_millis(1))
}

/// Scans every worker's heartbeat at `now`; returns how many are stuck.
pub(crate) fn scan(pool: &PoolShared, now: u64) -> usize {
    let limit = micros(pool.stuck_after);
    let mut stuck = 0;
    for worker in pool.workers.iter() {
        worker.sweep_mark();
        let Some(beat) = worker.heartbeat.read() else {
            continue;
        };
        let elapsed = if beat.hook() {
            worker.hook_elapsed(beat.number(), now)
        } else {
            now.saturating_sub(beat.start())
        };
        if elapsed <= limit {
            continue;
        }
        stuck += 1;
        worker.mark_stuck(beat.word);
        // The poll may have ended between the read and the mark, after the
        // worker looked for its mark: a worker no longer in that poll is not
        // left marked, or an idle worker would be routed around for good.
        if worker.heartbeat.word() != Some(beat.word) {
            worker.unmark(beat.word);
        }
        if beat.hook() {
            if worker.hook_reported.swap(beat.word, Ordering::AcqRel) != beat.word {
                worker
                    .counters
                    .long_hook_polls
                    .fetch_add(1, Ordering::Relaxed);
                report_long_hook(worker.id.index, elapsed, pool.stuck_after);
            }
        } else {
            let task = beat.task & !REPORTED;
            if beat.task & REPORTED == 0 && worker.reported.swap(task, Ordering::AcqRel) != task {
                worker.counters.long_polls.fetch_add(1, Ordering::Relaxed);
                report_long_poll(worker.id.index, beat.name, elapsed, pool.stuck_after);
            }
        }
        while let Some(job) = worker.take_free() {
            match pool.reroute(job, worker) {
                Ok(()) => {
                    worker.counters.moved_out.fetch_add(1, Ordering::Relaxed);
                }
                Err(job) => {
                    worker.push(job);
                    break;
                }
            }
        }
    }
    stuck
}
