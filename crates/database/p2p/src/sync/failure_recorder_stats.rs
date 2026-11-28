//! What the push failure recorder's lanes hold and do, for [`super::SyncStatus`].
//!
//! The recorder (in the P2P adapter) owns the lanes; the coordinator owns these
//! counters so a backed-up lane shows in the sync status beside the push
//! backlog that feeds it.

use std::sync::atomic::{AtomicU64, Ordering};

/// Lanes the failure recorder runs at once. A peer's records all go to one
/// lane, so they stay in the order they were sent and never run beside each
/// other (`RetryRecorderLanes.tla`).
pub const FAILURE_RECORDER_LANES: usize = 16;

/// Records a recorder lane holds waiting, and takes in one round.
pub const FAILURE_RECORDER_LANE_CAPACITY: usize = 256;

/// Live counters the recorder updates as it works.
#[derive(Default)]
pub struct FailureRecorderStats {
    lane_depth: [AtomicU64; FAILURE_RECORDER_LANES],
    lane_depth_high_water: AtomicU64,
    dispatcher_waits_total: AtomicU64,
    runs_total: AtomicU64,
    run_records_total: AtomicU64,
    largest_run: AtomicU64,
    failed_runs_total: AtomicU64,
    failed_records_total: AtomicU64,
}

/// A point-in-time copy of [`FailureRecorderStats`].
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct FailureRecorderSnapshot {
    /// Records waiting in each lane, counting one the dispatcher holds for a
    /// full lane.
    pub lane_depth: Vec<u64>,
    /// The most records any one lane has held waiting.
    pub lane_depth_high_water: u64,
    /// Times the dispatcher found a lane full and waited for it to drain.
    pub dispatcher_waits_total: u64,
    /// Runs of a peer's records recorded as one shared transaction.
    pub runs_total: u64,
    /// Records in those runs; divided by `runs_total`, the mean run size.
    pub run_records_total: u64,
    /// The most records one run held.
    pub largest_run: u64,
    /// Runs whose shared transaction failed, so each record ran on its own.
    pub failed_runs_total: u64,
    /// Records whose own write failed, answered with no marker.
    pub failed_records_total: u64,
}

impl FailureRecorderStats {
    /// The dispatcher has a record for `lane`, counted before the lane can
    /// take it.
    pub fn queued(&self, lane: usize) {
        let depth = self.lane_depth[lane].fetch_add(1, Ordering::Relaxed) + 1;
        self.lane_depth_high_water
            .fetch_max(depth, Ordering::Relaxed);
    }

    /// `lane` handed `count` records to a round.
    pub fn taken(&self, lane: usize, count: usize) {
        self.lane_depth[lane].fetch_sub(count as u64, Ordering::Relaxed);
    }

    /// The dispatcher found a lane full and waits for room.
    pub fn dispatcher_waited(&self) {
        self.dispatcher_waits_total.fetch_add(1, Ordering::Relaxed);
    }

    /// A run of `records` records went to one shared transaction, which
    /// failed when `failed`.
    pub fn ran(&self, records: usize, failed: bool) {
        self.runs_total.fetch_add(1, Ordering::Relaxed);
        self.run_records_total
            .fetch_add(records as u64, Ordering::Relaxed);
        self.largest_run
            .fetch_max(records as u64, Ordering::Relaxed);
        if failed {
            self.failed_runs_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// A record's own write failed.
    pub fn record_failed(&self) {
        self.failed_records_total.fetch_add(1, Ordering::Relaxed);
    }

    /// The counters now.
    pub fn snapshot(&self) -> FailureRecorderSnapshot {
        FailureRecorderSnapshot {
            lane_depth: self
                .lane_depth
                .iter()
                .map(|depth| depth.load(Ordering::Relaxed))
                .collect(),
            lane_depth_high_water: self.lane_depth_high_water.load(Ordering::Relaxed),
            dispatcher_waits_total: self.dispatcher_waits_total.load(Ordering::Relaxed),
            runs_total: self.runs_total.load(Ordering::Relaxed),
            run_records_total: self.run_records_total.load(Ordering::Relaxed),
            largest_run: self.largest_run.load(Ordering::Relaxed),
            failed_runs_total: self.failed_runs_total.load(Ordering::Relaxed),
            failed_records_total: self.failed_records_total.load(Ordering::Relaxed),
        }
    }
}
