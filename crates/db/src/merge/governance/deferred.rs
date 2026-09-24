use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use cid::Cid;
use defra_core::merge::MergeBlock;
use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use rapidhash::fast::RandomState;

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

type CidSet = HopscotchMap<Cid, (), RandomState>;

/// Deferred composites indexed by the inputs their verdict awaits.
///
/// Node-local and in memory, and holding only what a verdict named: a defer
/// naming nothing, one past [`MAX_DEFERRED_COMPOSITES`], and everything
/// indexed before a restart are all absent from it. The index is therefore an
/// arrival fast path, not the record of what is owed; that is the blockstore's
/// unmerged set, which the governance sweep walks.
///
/// Lock-free, as every shared structure in this crate is. Each operation is
/// atomic per map, not across them, and the one thing that must never happen
/// twice, re-driving a composite that is already queued, is decided by a
/// single `insert_if_absent` on `queued`. What a race between a defer and a
/// release can cost is only the fast path for that composite: its waiter is
/// filed after the arrival looked, or into a set the arrival has already
/// taken, and the sweep re-judges it at its interval, exactly as it does for
/// a composite the index never held.
pub(crate) struct DeferredMerges {
    /// Each deferred composite, with the keys it is filed under.
    entries: HopscotchMap<Cid, Entry, RandomState>,
    /// The composites awaiting each input.
    waiters: HopscotchMap<WaitKey, Arc<CidSet>, RandomState>,
    /// Released composites awaiting re-drive, in release order.
    ready: SegQueue<MergeBlock>,
    /// Composites in `ready` or being re-driven: the gate arrival re-drive and
    /// the sweep share, so neither merges a composite the other has taken.
    queued: CidSet,
    /// Entries held, counted so the capacity is a bound and not an estimate.
    held: AtomicUsize,
    /// Overridden only by tests, to reach the at-capacity path without
    /// indexing [`MAX_DEFERRED_COMPOSITES`] composites first. Zero means the
    /// production value.
    capacity: AtomicUsize,
}

#[derive(Clone)]
struct Entry {
    block: MergeBlock,
    awaiting: Vec<WaitKey>,
}

impl Default for DeferredMerges {
    fn default() -> Self {
        Self {
            entries: HopscotchMap::with_hasher(RandomState::default()),
            waiters: HopscotchMap::with_hasher(RandomState::default()),
            ready: SegQueue::new(),
            queued: CidSet::with_hasher(RandomState::default()),
            held: AtomicUsize::new(0),
            capacity: AtomicUsize::new(0),
        }
    }
}

impl DeferredMerges {
    /// The capacity a test asked for, or the production one.
    pub(crate) fn set_capacity(&self, capacity: usize) {
        self.capacity.store(capacity, Ordering::Release);
    }

    fn capacity(&self) -> usize {
        match self.capacity.load(Ordering::Acquire) {
            0 => MAX_DEFERRED_COMPOSITES,
            capacity => capacity,
        }
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
        let cid = block.cid;
        if self.queued.insert_if_absent(cid, ()).is_some() {
            return false;
        }
        let queued = match self.take_entry(&cid) {
            Some(entry) => entry.block,
            None => block,
        };
        self.ready.push(queued);
        true
    }

    /// `block.block_data` is left empty: re-drive reloads the bytes from the
    /// blockstore.
    pub(crate) fn defer(&self, block: MergeBlock, awaiting: Vec<WaitKey>) {
        let cid = block.cid;
        self.take_entry(&cid);
        if awaiting.is_empty() {
            return;
        }
        // Reserve the slot before filing anything, so the bound holds under
        // concurrent defers.
        if self.held.fetch_add(1, Ordering::AcqRel) >= self.capacity() {
            self.held.fetch_sub(1, Ordering::AcqRel);
            return;
        }
        let mut indexed = Vec::with_capacity(awaiting.len());
        for dependency in awaiting.into_iter().take(MAX_AWAITED_PER_COMPOSITE) {
            let waiters = self.waiters.get_or_insert(
                dependency.clone(),
                Arc::new(CidSet::with_hasher(RandomState::default())),
            );
            if waiters.len() < MAX_WAITERS_PER_DEPENDENCY
                && waiters.insert_if_absent(cid, ()).is_none()
            {
                indexed.push(dependency);
            }
        }
        if indexed.is_empty() {
            self.held.fetch_sub(1, Ordering::AcqRel);
            return;
        }
        if let Some(previous) = self.entries.insert(
            cid,
            Entry {
                block,
                awaiting: indexed,
            },
        ) {
            // A concurrent defer of the same composite filed first; its
            // entry is the one displaced, and its slot is released.
            self.held.fetch_sub(1, Ordering::AcqRel);
            self.unindex(&cid, &previous.awaiting);
        }
    }

    pub(crate) fn release(&self, merged: impl IntoIterator<Item = WaitKey>) {
        for dependency in merged {
            let Some(waiters) = self.waiters.remove(&dependency) else {
                continue;
            };
            for (waiter, ()) in waiters.iter() {
                // Whoever removes the entry owns the composite; a release on
                // another of its keys at the same time finds nothing here.
                let Some(entry) = self.take_entry(&waiter) else {
                    continue;
                };
                if self.queued.insert_if_absent(waiter, ()).is_none() {
                    self.ready.push(entry.block);
                }
            }
        }
    }

    pub(crate) fn take_ready(&self) -> Option<MergeBlock> {
        let block = self.ready.pop()?;
        self.queued.remove(&block.cid);
        Some(block)
    }

    /// Whether anything is waiting at all, so a path that could release a
    /// waiter pays nothing when nothing is deferred.
    pub(crate) fn has_waiters(&self) -> bool {
        !self.waiters.is_empty()
    }

    /// Whether any deferred composite awaits an immutable field value, so a
    /// merge needs to read its field values to release waiters.
    pub(crate) fn awaits_fields(&self) -> bool {
        self.waiters
            .keys()
            .any(|key| matches!(key, WaitKey::ImmutableField { .. }))
    }

    pub(crate) fn has_ready(&self) -> bool {
        !self.ready.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Remove a composite's entry and its waiters, releasing its slot.
    fn take_entry(&self, cid: &Cid) -> Option<Entry> {
        let entry = self.entries.remove(cid)?;
        self.held.fetch_sub(1, Ordering::AcqRel);
        self.unindex(cid, &entry.awaiting);
        Some(entry)
    }

    fn unindex(&self, cid: &Cid, awaiting: &[WaitKey]) {
        for dependency in awaiting {
            let Some(waiters) = self.waiters.get(dependency) else {
                continue;
            };
            waiters.remove(cid);
            if waiters.is_empty() {
                // A defer filing under this key between the check and the
                // removal loses its fast path, not its composite.
                self.waiters.remove(dependency);
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn cid(n: u64) -> Cid {
        use cid::multihash::Multihash;
        let digest = sha2::Sha256::digest(n.to_le_bytes());
        Cid::new_v1(0x71, Multihash::wrap(0x12, &digest).unwrap())
    }
    use sha2::Digest as _;

    fn block(n: u64) -> MergeBlock {
        MergeBlock {
            cid: cid(n),
            block_data: Default::default(),
            doc_id: String::new(),
            collection_id: String::new(),
            creator: String::new(),
            sender_peer: None,
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
            verified_creator: None,
        }
    }

    fn key(n: u64) -> WaitKey {
        WaitKey::Composite(cid(1_000_000 + n))
    }

    /// Defers and releases race from several threads. Whatever interleaving
    /// happens, a composite is never handed out twice while queued, and the
    /// slot count matches the entries left.
    #[test]
    fn concurrent_defer_and_release_never_double_queue() {
        let index = Arc::new(DeferredMerges::default());
        let threads: Vec<_> = (0..8u64)
            .map(|t| {
                let index = Arc::clone(&index);
                std::thread::spawn(move || {
                    for i in 0..500u64 {
                        let n = (t * 500 + i) % 64;
                        index.defer(block(n), vec![key(n % 8), key((n + 1) % 8)]);
                        index.release([key((i + t) % 8)]);
                        if i % 3 == 0 {
                            index.enqueue_ready(block(n));
                        }
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let mut seen = rapidhash::RapidHashSet::default();
        while let Some(block) = index.take_ready() {
            assert!(seen.insert(block.cid), "{} was queued twice", block.cid);
        }
        assert_eq!(index.held.load(Ordering::Acquire), index.len());
    }
}
