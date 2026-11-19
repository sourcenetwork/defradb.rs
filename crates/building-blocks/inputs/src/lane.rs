//! One group's batching lane: the keys waiting for a round trip, and how many
//! round trips the group has in flight.

use std::sync::atomic::{AtomicUsize, Ordering};

use regolith::sync::SegQueue;

pub(crate) struct Lane<K>
where
    K: 'static,
{
    waiting: SegQueue<K>,
    active: AtomicUsize,
}

impl<K> Lane<K>
where
    K: 'static,
{
    pub(crate) fn new() -> Self {
        Self {
            waiting: SegQueue::new(),
            active: AtomicUsize::new(0),
        }
    }

    pub(crate) fn push(&self, key: K) {
        self.waiting.push(key);
    }

    /// Takes a round-trip slot if fewer than `max` are in flight.
    pub(crate) fn claim(&self, max: usize) -> bool {
        self.active
            .try_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < max).then_some(active + 1)
            })
            .is_ok()
    }

    pub(crate) fn release(&self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }

    /// The oldest waiting keys, at most `max`.
    pub(crate) fn take(&self, max: usize) -> Vec<K> {
        let mut batch = Vec::with_capacity(max.min(self.waiting.len()));
        while batch.len() < max {
            match self.waiting.pop() {
                Some(key) => batch.push(key),
                None => break,
            }
        }
        batch
    }

    pub(crate) fn waiting(&self) -> usize {
        self.waiting.len()
    }

    pub(crate) fn has_waiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.active.load(Ordering::Acquire) == 0 && self.waiting.is_empty()
    }
}
