use std::collections::VecDeque;
use std::sync::Mutex;

use cid::Cid;
use defra_core::merge::MergeBlock;
use rapidhash::{RapidHashMap, RapidHashSet};

use super::WaitKey;

/// Most deferred composites indexed at once. Beyond it a deferred composite is
/// left to the replication retry clock instead of being re-driven on arrival.
pub const MAX_DEFERRED_COMPOSITES: usize = 16_384;
/// Most composites indexed under one awaited input.
pub const MAX_WAITERS_PER_DEPENDENCY: usize = 1_024;
/// Most awaited inputs indexed for one deferred composite; the rest are
/// ignored, leaving the composite to the retry clock if none of the indexed
/// ones arrives.
pub const MAX_AWAITED_PER_COMPOSITE: usize = 64;
/// Most released composites re-driven after one merge; the rest wait for the
/// next merge to drain them.
pub const REDRIVE_BUDGET: usize = 256;

/// Deferred composites indexed by the inputs their verdict awaits.
///
/// Node-local and in memory: after a restart the replication retry clock
/// re-merges each deferred composite, which re-indexes it.
#[derive(Default)]
pub(crate) struct DeferredMerges {
    inner: Mutex<Inner>,
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

    /// `block.block_data` is left empty: re-drive reloads the bytes from the
    /// blockstore.
    pub(crate) fn defer(&self, block: MergeBlock, awaiting: Vec<WaitKey>) {
        let mut inner = self.lock();
        let cid = block.cid;
        if let Some(previous) = inner.entries.remove(&cid) {
            inner.unindex(&cid, &previous.awaiting);
        }
        if awaiting.is_empty() || inner.entries.len() >= MAX_DEFERRED_COMPOSITES {
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
