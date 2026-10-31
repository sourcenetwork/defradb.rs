//! Go Shinzo's merge and purge counters (#5146): what one node merged,
//! refused and purged since the last report. Each database owns one, so two
//! nodes in one process never mix their counts. Read and reset by the
//! node's 30-second report, which logs `merge stats` and `purge stats`.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::merge::merge_handler::MergeError;
use crate::retention::RetentionRefusal;

/// Why a merge dropped an event (stored nothing), by Go's `merge drops`
/// names. Go's `contextDone` has no counterpart: a Rust merge is not
/// cancelled midway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropCause {
    MissingBlock,
    UniqueIndex,
    RetryExhausted,
    CollectionNotFound,
    Other,
}

impl DropCause {
    const ALL: [Self; 5] = [
        Self::MissingBlock,
        Self::UniqueIndex,
        Self::RetryExhausted,
        Self::CollectionNotFound,
        Self::Other,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::MissingBlock => "missingBlock",
            Self::UniqueIndex => "uniqueIndex",
            Self::RetryExhausted => "retryExhausted",
            Self::CollectionNotFound => "collectionNotFound",
            Self::Other => "other",
        }
    }

    /// The cause of a merge that failed with `error`, or `None` when the
    /// failure is not a drop: a retention refusal is counted as a rejection.
    pub fn of(error: &MergeError) -> Option<Self> {
        use crate::error::Error;
        let text = error.to_string();
        Some(match error {
            MergeError::RetentionRefused(_) => return None,
            MergeError::UniqueConstraintViolation(_) => Self::UniqueIndex,
            MergeError::RetryRejected(reason) if reason.contains("unique") => Self::UniqueIndex,
            MergeError::Database(Error::Storage(storage::corekv::Error::TxnRetryExhausted(_))) => {
                Self::RetryExhausted
            }
            MergeError::Database(Error::CollectionNotFound(_)) => Self::CollectionNotFound,
            MergeError::MissingMetadata(_) if text.contains("Collection not found") => {
                Self::CollectionNotFound
            }
            _ if text.contains("not found in blockstore") => Self::MissingBlock,
            _ => Self::Other,
        })
    }
}

/// One node's merge and purge counters.
#[derive(Debug, Default)]
pub struct MergeStats {
    creates: AtomicU64,
    updates: AtomicU64,
    txn_conflicts: AtomicU64,
    rejected_below_floor: AtomicU64,
    rejected_no_height: AtomicU64,
    deleted: AtomicU64,
    drops: [AtomicU64; DropCause::ALL.len()],
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
    /// Non-zero drop causes, by Go name.
    pub drops: Vec<(&'static str, u64)>,
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

    /// A merge dropped its event: it failed, storing nothing.
    pub fn record_drop(&self, error: &MergeError) {
        if let Some(cause) = DropCause::of(error) {
            self.drops[cause as usize].fetch_add(1, Ordering::Relaxed);
        }
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
            drops: DropCause::ALL
                .iter()
                .map(|cause| {
                    (
                        cause.name(),
                        self.drops[*cause as usize].swap(0, Ordering::Relaxed),
                    )
                })
                .filter(|(_, n)| *n > 0)
                .collect(),
        }
    }
}

impl MergeReport {
    /// Logs Go's `merge stats` line when any merge count is non-zero, its
    /// `purge stats` line when anything was purged, and its `merge drops`
    /// error line when any event was dropped.
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
        if !self.drops.is_empty() {
            tracing::error!(causes = ?self.drops, "merge drops");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_failures_map_to_go_drop_causes() {
        let cause = |error: MergeError| DropCause::of(&error);
        assert_eq!(
            cause(MergeError::UniqueConstraintViolation("taken".into())),
            Some(DropCause::UniqueIndex)
        );
        assert_eq!(
            cause(MergeError::Storage(
                "Linked block bafy not found in blockstore".into()
            )),
            Some(DropCause::MissingBlock)
        );
        assert_eq!(
            cause(MergeError::MissingMetadata(
                "Collection not found for collection_version_id: v9".into()
            )),
            Some(DropCause::CollectionNotFound)
        );
        assert_eq!(
            cause(MergeError::Database(
                crate::error::Error::CollectionNotFound("Users".into())
            )),
            Some(DropCause::CollectionNotFound)
        );
        assert_eq!(
            cause(MergeError::BlockDecode("bad".into())),
            Some(DropCause::Other)
        );
        assert_eq!(
            cause(MergeError::RetentionRefused(RetentionRefusal::BelowFloor)),
            None,
            "a retention refusal is a rejection, not a drop"
        );
    }

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
                drops: Vec::new(),
            }
        );
        assert_eq!(stats.drain(), MergeReport::default());
    }
}
