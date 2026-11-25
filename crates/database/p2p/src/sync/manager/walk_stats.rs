//! Counters for local missing-link walks (`links::find_all_missing_links`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// What every finished walk adds: how many children it looked at, how many
/// pages it yielded after, and how long it took, with the largest seen.
#[derive(Debug, Default)]
pub(crate) struct MissingLinkWalkStats {
    completed: AtomicU64,
    preempted: AtomicU64,
    errors: AtomicU64,
    blocks: AtomicU64,
    blocks_max: AtomicU64,
    pages: AtomicU64,
    micros: AtomicU64,
    micros_max: AtomicU64,
}

/// A point-in-time view of the node's missing-link walk counters, carried by
/// `SyncStatus` as `missing_link_walks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct MissingLinkWalkSnapshot {
    /// Walks that read every reachable unmerged block.
    pub completed: u64,
    /// Walks a fetch stopped at a page boundary past its quantum.
    pub preempted: u64,
    /// Walks that failed on a local read or parse.
    pub errors: u64,
    /// Children looked at, summed over finished walks.
    pub blocks: u64,
    /// The most children one walk looked at.
    pub blocks_max: u64,
    /// Pages after which a walk gave its core to other tasks.
    pub pages: u64,
    /// Microseconds spent walking, summed over finished walks.
    pub micros: u64,
    /// The longest one walk took, in microseconds.
    pub micros_max: u64,
}

/// How one walk ended, for [`MissingLinkWalkStats::record`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WalkEnding {
    Completed,
    Preempted,
    Failed,
}

impl MissingLinkWalkStats {
    pub(crate) fn record(&self, ending: WalkEnding, blocks: u64, pages: u64, elapsed: Duration) {
        match ending {
            WalkEnding::Completed => self.completed.fetch_add(1, Ordering::Relaxed),
            WalkEnding::Preempted => self.preempted.fetch_add(1, Ordering::Relaxed),
            // The caller that acts on the failure counts it (`record_error`).
            WalkEnding::Failed => 0,
        };
        let micros = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        self.blocks.fetch_add(blocks, Ordering::Relaxed);
        self.blocks_max.fetch_max(blocks, Ordering::Relaxed);
        self.pages.fetch_add(pages, Ordering::Relaxed);
        self.micros.fetch_add(micros, Ordering::Relaxed);
        self.micros_max.fetch_max(micros, Ordering::Relaxed);
    }

    /// A caller took a walk, or the root read before it, as failed.
    pub(crate) fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn snapshot(&self) -> MissingLinkWalkSnapshot {
        MissingLinkWalkSnapshot {
            completed: self.completed.load(Ordering::Relaxed),
            preempted: self.preempted.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            blocks: self.blocks.load(Ordering::Relaxed),
            blocks_max: self.blocks_max.load(Ordering::Relaxed),
            pages: self.pages.load(Ordering::Relaxed),
            micros: self.micros.load(Ordering::Relaxed),
            micros_max: self.micros_max.load(Ordering::Relaxed),
        }
    }
}
