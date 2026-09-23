use std::collections::VecDeque;
use std::sync::Mutex;

use cid::Cid;
use defra_core::merge::MergeBlock;
use rapidhash::{RapidHashMap, RapidHashSet};

use super::WaitKey;

/// Most deferred composites indexed at once. Beyond it a deferred composite is
/// not re-driven on arrival; the governance sweep re-judges it instead.
pub const MAX_DEFERRED_COMPOSITES: usize = 16_384;
/// Most composites indexed under one awaited input.
pub const MAX_WAITERS_PER_DEPENDENCY: usize = 1_024;
/// Most awaited inputs indexed for one deferred composite; the rest are
/// ignored; if none of the indexed ones arrives, the governance sweep is what
/// re-judges the composite.
pub const MAX_AWAITED_PER_COMPOSITE: usize = 64;
/// Most released composites re-driven after one merge; the rest wait for the
/// next merge to drain them.
pub const REDRIVE_BUDGET: usize = 256;

/// Deferred composites indexed by the inputs their verdict awaits.
///
/// Node-local and in memory, and holding only what a verdict named: a defer
/// naming nothing, one past [`MAX_DEFERRED_COMPOSITES`], and everything
/// indexed before a restart are all absent from it. The index is therefore an
/// arrival fast path, not the record of what is owed; that is the blockstore's
/// unmerged set, which the governance sweep walks.
#[derive(Default)]
pub(crate) struct DeferredMerges {
    inner: Mutex<Inner>,
    /// Overridden only by tests, to reach the at-capacity path without
    /// indexing [`MAX_DEFERRED_COMPOSITES`] composites first.
    capacity: Mutex<Option<usize>>,
}

#[derive(Default)]
struct Inner {
    waiters: RapidHashMap<WaitKey, RapidHashSet<Cid>>,
    entries: RapidHashMap<Cid, Entry>,
    ready: VecDeque<MergeBlock>,
    queued: RapidHashSet<Cid>,
}

struct Entry {
    block: MergeBlock,
    awaiting: Vec<WaitKey>,
}

impl DeferredMerges {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The capacity a test asked for, or the production one.
    pub(crate) fn set_capacity(&self, capacity: usize) {
        *self
            .capacity
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(capacity);
    }

    fn capacity(&self) -> usize {
        self.capacity
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .unwrap_or(MAX_DEFERRED_COMPOSITES)
    }

    /// Queue a composite for re-drive without an awaited input to release it.
    /// The governance sweep's entry point, sharing the `queued` gate with
    /// arrival re-drive so the two cannot merge one composite twice.
    ///
    /// A composite the index still holds is queued too, and leaves the index
    /// as if its input had arrived. It is not left to its arrival: an entry
    /// can await something no arrival releases, such as a field block's CID,
    /// and would otherwise wait forever. The indexed entry's own carrier is
    /// preferred over the caller's, since the sweep has none to give.
    pub(crate) fn enqueue_ready(&self, block: MergeBlock) -> bool {
        let mut inner = self.lock();
        let cid = block.cid;
        if !inner.queued.insert(cid) {
            return false;
        }
        let queued = match inner.entries.remove(&cid) {
            Some(entry) => {
                inner.unindex(&cid, &entry.awaiting);
                entry.block
            }
            None => block,
        };
        inner.ready.push_back(queued);
        true
    }

    /// `block.block_data` is left empty: re-drive reloads the bytes from the
    /// blockstore.
    pub(crate) fn defer(&self, block: MergeBlock, awaiting: Vec<WaitKey>) {
        let capacity = self.capacity();
        let mut inner = self.lock();
        let cid = block.cid;
        if let Some(previous) = inner.entries.remove(&cid) {
            inner.unindex(&cid, &previous.awaiting);
        }
        if awaiting.is_empty() || inner.entries.len() >= capacity {
            return;
        }
        let mut indexed = Vec::with_capacity(awaiting.len());
        for dependency in awaiting.into_iter().take(MAX_AWAITED_PER_COMPOSITE) {
            let waiters = inner.waiters.entry(dependency.clone()).or_default();
            if waiters.len() < MAX_WAITERS_PER_DEPENDENCY && waiters.insert(cid) {
                indexed.push(dependency);
            }
        }
        if !indexed.is_empty() {
            inner.entries.insert(
                cid,
                Entry {
                    block,
                    awaiting: indexed,
                },
            );
        }
    }

    pub(crate) fn release(&self, merged: impl IntoIterator<Item = WaitKey>) {
        let mut inner = self.lock();
        for dependency in merged {
            let Some(waiters) = inner.waiters.remove(&dependency) else {
                continue;
            };
            for waiter in waiters {
                let Some(entry) = inner.entries.remove(&waiter) else {
                    continue;
                };
                inner.unindex(&waiter, &entry.awaiting);
                if inner.queued.insert(waiter) {
                    inner.ready.push_back(entry.block);
                }
            }
        }
    }

    pub(crate) fn take_ready(&self) -> Option<MergeBlock> {
        let mut inner = self.lock();
        let block = inner.ready.pop_front()?;
        inner.queued.remove(&block.cid);
        Some(block)
    }

    /// Whether anything is waiting at all, so a path that could release a
    /// waiter pays nothing when nothing is deferred.
    pub(crate) fn has_waiters(&self) -> bool {
        !self.lock().waiters.is_empty()
    }

    /// Whether any deferred composite awaits an immutable field value, so a
    /// merge needs to read its field values to release waiters.
    pub(crate) fn awaits_fields(&self) -> bool {
        self.lock()
            .waiters
            .keys()
            .any(|key| matches!(key, WaitKey::ImmutableField { .. }))
    }

    pub(crate) fn has_ready(&self) -> bool {
        !self.lock().ready.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.lock().entries.len()
    }
}

impl Inner {
    fn unindex(&mut self, cid: &Cid, awaiting: &[WaitKey]) {
        for dependency in awaiting {
            if let Some(waiters) = self.waiters.get_mut(dependency) {
                waiters.remove(cid);
                if waiters.is_empty() {
                    self.waiters.remove(dependency);
                }
            }
        }
    }
}
