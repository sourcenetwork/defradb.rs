//! Asks for a drain of a deferred index once a write that owes it commits.

use std::sync::Arc;

use rapidhash::fast::RandomState;
use rapidhash::{HashSetExt, RapidHashSet};
use regolith::sync::{HopscotchMap, Notify};

use crate::index::vector::pending::Backlogs;

/// One deferred index: its collection's short id and its id.
pub(crate) type IndexRef = (u32, u32);

/// The drains a database owes, shared by every handle of it.
#[derive(Clone)]
pub struct DrainRequests(Arc<Inner>);

struct Inner {
    /// Keyed by index, so it never holds more entries than there are
    /// deferred indexes, however many commits ask.
    asked: HopscotchMap<IndexRef, (), RandomState>,
    wake: Notify,
    /// How many writes each deferred vector index holds pending.
    backlogs: Backlogs,
}

impl Default for DrainRequests {
    fn default() -> Self {
        Self(Arc::new(Inner {
            asked: HopscotchMap::with_hasher(RandomState::default()),
            wake: Notify::default(),
            backlogs: Backlogs::default(),
        }))
    }
}

impl DrainRequests {
    pub(crate) fn request(&self, index: IndexRef) {
        self.0.asked.insert(index, ());
        self.0.wake.notify_one();
    }

    /// Keeps `index` for a later pass without waking the drain.
    pub(crate) fn defer(&self, index: IndexRef) {
        self.0.asked.insert(index, ());
    }

    /// Waits until a drain is asked for.
    pub(crate) async fn requested(&self) {
        self.0.wake.notified().await;
    }

    /// Every index asked for since the last take.
    pub(crate) fn take(&self) -> RapidHashSet<IndexRef> {
        let mut indexes = RapidHashSet::new();
        for index in self.0.asked.keys().collect::<Vec<_>>() {
            if self.0.asked.remove(&index).is_some() {
                indexes.insert(index);
            }
        }
        indexes
    }

    /// The pending counts of the deferred vector indexes.
    pub fn backlogs(&self) -> &Backlogs {
        &self.0.backlogs
    }

    /// How many indexes wait for a drain.
    pub fn len(&self) -> usize {
        self.0.asked.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
