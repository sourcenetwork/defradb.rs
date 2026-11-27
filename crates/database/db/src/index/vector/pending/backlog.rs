//! How many pending writes each deferred vector index holds, as this node
//! counts them.
//!
//! The count only decides whether a write defers or updates the graph itself,
//! and both are correct, so it is kept in memory and may run high: a write
//! that replaces a pending entry counts again until the next drain that
//! empties the set. It starts from the stored count when a node opens.

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
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
    count_state: AtomicU8,
}

const COUNT_UNSTARTED: u8 = 0;
const COUNTING: u8 = 1;
const COUNTED: u8 = 2;

/// The exclusive right to add the stored count to a [`Backlog`].
pub(crate) struct FirstCount<'a> {
    backlog: &'a Backlog,
    complete: bool,
}

/// What a drain found when asking who should add the stored count.
pub(crate) enum FirstCountClaim<'a> {
    Claimed(FirstCount<'a>),
    Initializing,
    Ready,
}

impl FirstCount<'_> {
    /// Keeps the stored count from being added again.
    pub(crate) fn complete(mut self) {
        self.complete = true;
        self.backlog.count_state.store(COUNTED, Ordering::Release);
    }
}

impl Drop for FirstCount<'_> {
    fn drop(&mut self) {
        if !self.complete {
            self.backlog
                .count_state
                .store(COUNT_UNSTARTED, Ordering::Release);
        }
    }
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

    /// Claims the right to add the stored count once. A failed or cancelled
    /// scan releases the claim so a later drain can try again.
    pub(crate) fn first_count(&self) -> FirstCountClaim<'_> {
        match self.count_state.compare_exchange(
            COUNT_UNSTARTED,
            COUNTING,
            Ordering::Acquire,
            Ordering::Acquire,
        ) {
            Ok(_) => FirstCountClaim::Claimed(FirstCount {
                backlog: self,
                complete: false,
            }),
            Err(COUNTING) => FirstCountClaim::Initializing,
            Err(_) => FirstCountClaim::Ready,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancelled_first_count_can_be_claimed_again() {
        let backlog = Backlog::default();
        let FirstCountClaim::Claimed(first) = backlog.first_count() else {
            panic!("the first count is free");
        };
        assert!(matches!(
            backlog.first_count(),
            FirstCountClaim::Initializing
        ));
        drop(first);

        let FirstCountClaim::Claimed(first) = backlog.first_count() else {
            panic!("the cancelled claim is free again");
        };
        first.complete();
        assert!(matches!(backlog.first_count(), FirstCountClaim::Ready));
    }
}
