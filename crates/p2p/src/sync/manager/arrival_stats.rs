//! Interval counters behind Go Shinzo's `p2p stats` report: every replicated
//! document ends merged, skipped (a deliberate non-merge) or dropped (a loss),
//! counted once at its final outcome, under a reason from a fixed set so a
//! peer cannot add report fields by varying its errors.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::sync::replication::ReplicationResult;

/// Why a document was dropped. Names match Go's `document drops` fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// The merge failed or was deterministically rejected.
    MergeFailed,
}

/// Why a document was skipped. Names match Go's `document skips` fields,
/// plus `rejectedByCRDT` and `other`, which Go has no outcome for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    AlreadyMerged,
    NoAccess,
    Filtered,
    RejectedByCrdt,
    Other,
}

impl DropReason {
    const ALL: [Self; 1] = [Self::MergeFailed];

    fn name(self) -> &'static str {
        match self {
            Self::MergeFailed => "mergeFailed",
        }
    }
}

impl SkipReason {
    const ALL: [Self; 5] = [
        Self::AlreadyMerged,
        Self::NoAccess,
        Self::Filtered,
        Self::RejectedByCrdt,
        Self::Other,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::AlreadyMerged => "alreadyMerged",
            Self::NoAccess => "noAccess",
            Self::Filtered => "filtered",
            Self::RejectedByCrdt => "rejectedByCRDT",
            Self::Other => "other",
        }
    }

    fn of(reason: &str) -> Self {
        let reason = reason.to_ascii_lowercase();
        if reason.contains("already merged") || reason.contains("already applied") {
            Self::AlreadyMerged
        } else if reason.contains("acp") || reason.contains("access") {
            Self::NoAccess
        } else if reason.contains("filter") {
            Self::Filtered
        } else if reason.contains("crdt") {
            Self::RejectedByCrdt
        } else {
            Self::Other
        }
    }
}

/// A document's final outcome, or `None` for a result that is not one (a
/// retryable skip the node will re-drive, a fetch that started).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    Merged,
    Skipped(SkipReason),
    Dropped(DropReason),
}

impl Arrival {
    pub fn of(result: &ReplicationResult) -> Option<Self> {
        match result {
            ReplicationResult::Merged { .. }
            | ReplicationResult::MergedButBroadcastFailed { .. }
            | ReplicationResult::MergedButNotMarked { .. } => Some(Self::Merged),
            ReplicationResult::Skipped {
                reason,
                terminal: true,
                ..
            } => Some(Self::Skipped(SkipReason::of(reason))),
            ReplicationResult::Quarantined { .. } | ReplicationResult::Failed { .. } => {
                Some(Self::Dropped(DropReason::MergeFailed))
            }
            _ => None,
        }
    }
}

/// One interval's counts, read and reset by [`ArrivalStats::drain`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ArrivalReport {
    pub docs_merged: u64,
    pub docs_skipped: u64,
    pub docs_dropped: u64,
    /// Inbound pushes this node received (`msgsIn`).
    pub msgs_in: u64,
    /// Inbound pushes refused because every request worker was busy
    /// (`droppedQueueFull`).
    pub dropped_queue_full: u64,
    /// Further `p2p stats` fields, by Go name, from sources outside this
    /// manager (the push queue, the inbound byte budget, the Shinzo wire).
    pub extra: Vec<(&'static str, u64)>,
    /// Non-zero drop reasons, by Go name.
    pub drops: Vec<(&'static str, u64)>,
    /// Non-zero skip reasons, by Go name.
    pub skips: Vec<(&'static str, u64)>,
}

impl ArrivalReport {
    /// The `p2p stats` fields, by Go name, in Go's order where Go has one.
    pub fn fields(&self) -> Vec<(&'static str, u64)> {
        let mut fields = vec![
            ("docsMerged", self.docs_merged),
            ("docsDropped", self.docs_dropped),
            ("docsSkipped", self.docs_skipped),
            ("msgsIn", self.msgs_in),
            ("droppedQueueFull", self.dropped_queue_full),
        ];
        fields.extend(self.extra.iter().copied());
        fields
    }

    /// Logs Go's `p2p stats` line, then `document drops` and `document skips`
    /// when they are non-empty.
    pub fn log(&self) {
        let fields: Vec<String> = self
            .fields()
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        tracing::info!(stats = %fields.join(" "), "p2p stats");
        if !self.drops.is_empty() {
            tracing::info!(reasons = ?self.drops, "document drops");
        }
        if !self.skips.is_empty() {
            tracing::info!(reasons = ?self.skips, "document skips");
        }
    }
}

/// Counts arrivals until the next report drains them.
#[derive(Debug, Default)]
pub struct ArrivalStats {
    merged: AtomicU64,
    msgs_in: AtomicU64,
    dropped_queue_full: AtomicU64,
    drops: [AtomicU64; DropReason::ALL.len()],
    skips: [AtomicU64; SkipReason::ALL.len()],
}

impl ArrivalStats {
    /// Count one inbound push; `admitted` is false when it was refused for
    /// want of a request worker.
    pub fn record_inbound(&self, admitted: bool) {
        self.msgs_in.fetch_add(1, Ordering::Relaxed);
        if !admitted {
            self.dropped_queue_full.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record(&self, result: &ReplicationResult) {
        match Arrival::of(result) {
            Some(Arrival::Merged) => self.merged.fetch_add(1, Ordering::Relaxed),
            Some(Arrival::Skipped(reason)) => {
                self.skips[reason as usize].fetch_add(1, Ordering::Relaxed)
            }
            Some(Arrival::Dropped(reason)) => {
                self.drops[reason as usize].fetch_add(1, Ordering::Relaxed)
            }
            None => 0,
        };
    }

    /// This interval's counts; every counter starts again from zero.
    pub fn drain(&self) -> ArrivalReport {
        let drops: Vec<_> = DropReason::ALL
            .iter()
            .map(|r| (r.name(), self.drops[*r as usize].swap(0, Ordering::Relaxed)))
            .filter(|(_, n)| *n > 0)
            .collect();
        let skips: Vec<_> = SkipReason::ALL
            .iter()
            .map(|r| (r.name(), self.skips[*r as usize].swap(0, Ordering::Relaxed)))
            .filter(|(_, n)| *n > 0)
            .collect();
        ArrivalReport {
            docs_merged: self.merged.swap(0, Ordering::Relaxed),
            docs_dropped: drops.iter().map(|(_, n)| n).sum(),
            docs_skipped: skips.iter().map(|(_, n)| n).sum(),
            msgs_in: self.msgs_in.swap(0, Ordering::Relaxed),
            dropped_queue_full: self.dropped_queue_full.swap(0, Ordering::Relaxed),
            extra: Vec::new(),
            drops,
            skips,
        }
    }
}

#[cfg(test)]
mod tests {
    use cid::Cid;

    use super::*;

    fn skipped(reason: &str, terminal: bool) -> ReplicationResult {
        ReplicationResult::Skipped {
            cid: Cid::default(),
            doc_id: String::new(),
            collection_id: String::new(),
            reason: reason.into(),
            terminal,
        }
    }

    #[test]
    fn drops_and_skips_are_counted_apart_and_drained() {
        let stats = ArrivalStats::default();
        let failed = ReplicationResult::Failed {
            cid: Cid::default(),
            error: "boom".into(),
        };
        stats.record(&failed);
        stats.record(&failed);
        stats.record(&skipped("already merged", true));
        stats.record(&skipped("pending ACP", true));
        stats.record(&skipped("merge work budget yielded", false));
        stats.record(&ReplicationResult::ChannelClosed);

        let report = stats.drain();
        assert_eq!(report.docs_dropped, 2);
        assert_eq!(report.docs_skipped, 2);
        assert_eq!(report.drops, vec![("mergeFailed", 2)]);
        assert_eq!(report.skips, vec![("alreadyMerged", 1), ("noAccess", 1)]);
        assert_eq!(stats.drain(), ArrivalReport::default());
    }

    #[test]
    fn inbound_pushes_and_queue_full_refusals_are_reported_by_go_name() {
        let stats = ArrivalStats::default();
        stats.record_inbound(true);
        stats.record_inbound(true);
        stats.record_inbound(false);
        let mut report = stats.drain();
        report.extra.push(("queueDepth", 7));
        let fields = report.fields();
        assert!(fields.contains(&("msgsIn", 3)));
        assert!(fields.contains(&("droppedQueueFull", 1)));
        assert_eq!(fields.last(), Some(&("queueDepth", 7)));
        assert_eq!(stats.drain(), ArrivalReport::default());
    }
}
