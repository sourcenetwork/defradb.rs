//! Counters of what the runtime refused, lost to a panic or gave up on.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub(crate) struct Stats {
    refused: AtomicU64,
    panicked: AtomicU64,
    deadlines: AtomicU64,
}

impl Stats {
    pub(crate) fn refused(&self) {
        self.refused.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn panicked(&self) {
        self.panicked.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn deadline(&self) {
        self.deadlines.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn snapshot(&self, in_flight: usize) -> EdgeStats {
        EdgeStats {
            in_flight,
            refused: self.refused.load(Ordering::Relaxed),
            panicked: self.panicked.load(Ordering::Relaxed),
            deadlines: self.deadlines.load(Ordering::Relaxed),
        }
    }
}

/// What a runtime has done, read at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EdgeStats {
    /// Work admitted through this handle that has not finished.
    pub in_flight: usize,
    /// Work refused because the runtime or a scope stopped accepting.
    pub refused: u64,
    /// Work that panicked, each failing only its own completion.
    pub panicked: u64,
    /// Completions whose waiter gave up at the deadline.
    pub deadlines: u64,
}
