//! How many pending writes each deferred vector index holds, as this node
//! counts them.
//!
//! The count only decides whether a write defers or updates the graph itself,
//! and both are correct, so it is kept in memory and may run high: a write
//! that replaces a pending entry counts again until the next drain that
//! empties the set. It starts from the stored count when a node opens.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;

/// Pending writes an index holds before writes update the graph themselves
/// again, Go's `vectorMaintenancePendingLimit`. It bounds the set every search
/// scores; a writer faster than the drain slows to the inline cost instead.
pub const PENDING_LIMIT: u64 = 1000;

/// One deferred index's count.
#[derive(Debug, Default)]
pub struct Backlog {
    pending: AtomicU64,
    counted: AtomicBool,
}

impl Backlog {
    pub fn pending(&self) -> u64 {
        self.pending.load(Ordering::Relaxed)
    }

    /// Whether a write should update the graph itself rather than defer.
    pub fn over_limit(&self) -> bool {
        self.pending() >= PENDING_LIMIT
    }

    pub fn add(&self, writes: u64) {
        self.pending.fetch_add(writes, Ordering::Relaxed);
    }

    /// Never below zero: a count that ran high is cut, not wrapped.
    pub fn sub(&self, writes: u64) {
        let _ = self
            .pending
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |pending| {
                Some(pending.saturating_sub(writes))
            });
    }

    /// True the first time it is asked, so the stored count is added once.
    pub fn first_count(&self) -> bool {
        !self.counted.swap(true, Ordering::Relaxed)
    }
}

/// Every deferred index's count on this node.
pub struct Backlogs {
    by_index: HopscotchMap<(u32, u32), Arc<Backlog>, RandomState>,
}

impl Default for Backlogs {
    fn default() -> Self {
        Self {
            by_index: HopscotchMap::with_hasher(RandomState::default()),
        }
    }
}

impl Backlogs {
    pub fn get(&self, collection_short_id: u32, index_id: u32) -> Arc<Backlog> {
        let key = (collection_short_id, index_id);
        self.by_index
            .get(&key)
            .unwrap_or_else(|| self.by_index.get_or_insert(key, Arc::default()))
    }

    /// Forgets an index no longer kept deferred, such as a dropped one.
    pub fn forget(&self, collection_short_id: u32, index_id: u32) {
        self.by_index.remove(&(collection_short_id, index_id));
    }
}
