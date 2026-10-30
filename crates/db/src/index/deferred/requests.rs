//! Asks for a drain of a deferred index once a write that owes it commits.

use std::sync::Arc;

use rapidhash::{HashSetExt, RapidHashSet};
use regolith::sync::{Notify, SegQueue};

/// One deferred index: its collection's short id and its id.
pub(crate) type IndexRef = (u32, u32);

/// The drains a database owes, shared by every handle of it.
#[derive(Clone, Default)]
pub struct DrainRequests(Arc<Inner>);

#[derive(Default)]
struct Inner {
    queue: SegQueue<IndexRef>,
    wake: Notify,
}

impl DrainRequests {
    pub(crate) fn request(&self, index: IndexRef) {
        self.0.queue.push(index);
        self.0.wake.notify_one();
    }

    /// Waits until a drain is asked for.
    pub(crate) async fn requested(&self) {
        self.0.wake.notified().await;
    }

    /// Every index asked for since the last take, each once.
    pub(crate) fn take(&self) -> RapidHashSet<IndexRef> {
        let mut indexes = RapidHashSet::new();
        while let Some(index) = self.0.queue.pop() {
            indexes.insert(index);
        }
        indexes
    }
}
