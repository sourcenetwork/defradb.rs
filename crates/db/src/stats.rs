//! Go Shinzo's merge and purge counters (#5146): what one node merged,
//! refused and purged since the last report. Each database owns one, so two
//! nodes in one process never mix their counts. Read and reset by the
//! node's 30-second report, which logs `merge stats` and `purge stats`.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::retention::RetentionRefusal;

/// One node's merge and purge counters.
#[derive(Debug, Default)]
pub struct MergeStats {
    creates: AtomicU64,
    updates: AtomicU64,
    txn_conflicts: AtomicU64,
    rejected_below_floor: AtomicU64,
    rejected_no_height: AtomicU64,
    deleted: AtomicU64,
}

/// One interval's counts, by Go name.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MergeReport {
    pub creates: u64,
    pub updates: u64,
    pub txn_conflicts: u64,
    pub rejected_below_floor: u64,
    pub rejected_no_height: u64,
    pub deleted: u64,
}

impl MergeStats {
    /// A merge committed: a create when the document had no heads before.
    pub fn record_merge(&self, created: bool) {
        let counter = if created {
            &self.creates
        } else {
            &self.updates
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// A merge attempt was abandoned to a transaction conflict.
    pub fn record_txn_conflict(&self) {
        self.txn_conflicts.fetch_add(1, Ordering::Relaxed);
    }

    /// The retention rule refused a replicated document.
    pub fn record_retention_refusal(&self, refusal: RetentionRefusal) {
        let counter = match refusal {
            RetentionRefusal::BelowFloor => &self.rejected_below_floor,
            RetentionRefusal::NoHeight => &self.rejected_no_height,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// A purge committed, removing `count` documents.
    pub fn record_purged(&self, count: u64) {
        self.deleted.fetch_add(count, Ordering::Relaxed);
    }

    /// This interval's counts; every counter starts again from zero.
    pub fn drain(&self) -> MergeReport {
        MergeReport {
            creates: self.creates.swap(0, Ordering::Relaxed),
            updates: self.updates.swap(0, Ordering::Relaxed),
            txn_conflicts: self.txn_conflicts.swap(0, Ordering::Relaxed),
            rejected_below_floor: self.rejected_below_floor.swap(0, Ordering::Relaxed),
            rejected_no_height: self.rejected_no_height.swap(0, Ordering::Relaxed),
            deleted: self.deleted.swap(0, Ordering::Relaxed),
        }
    }
}

impl MergeReport {
    /// Logs Go's `merge stats` line when any merge count is non-zero, and
    /// its `purge stats` line when anything was purged.
    pub fn log(&self) {
        if self.creates
            + self.updates
            + self.txn_conflicts
            + self.rejected_below_floor
            + self.rejected_no_height
            > 0
        {
            tracing::info!(
                creates = self.creates,
                updates = self.updates,
                txnConflicts = self.txn_conflicts,
                rejectedBelowFloor = self.rejected_below_floor,
                rejectedNoHeight = self.rejected_no_height,
                "merge stats"
            );
        }
        if self.deleted > 0 {
            tracing::info!(deleted = self.deleted, "purge stats");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_go_under_their_names_and_reset_each_interval() {
        let stats = MergeStats::default();
        stats.record_merge(true);
        stats.record_merge(false);
        stats.record_merge(false);
        stats.record_txn_conflict();
        stats.record_retention_refusal(RetentionRefusal::BelowFloor);
        stats.record_retention_refusal(RetentionRefusal::NoHeight);
        stats.record_retention_refusal(RetentionRefusal::NoHeight);
        stats.record_purged(4);
        assert_eq!(
            stats.drain(),
            MergeReport {
                creates: 1,
                updates: 2,
                txn_conflicts: 1,
                rejected_below_floor: 1,
                rejected_no_height: 2,
                deleted: 4,
            }
        );
        assert_eq!(stats.drain(), MergeReport::default());
    }
}
