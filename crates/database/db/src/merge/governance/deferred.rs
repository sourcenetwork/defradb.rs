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
        if awaiting.is_empty() || !self.reserve() {
            return;
        }
        let indexed: Vec<WaitKey> = awaiting
            .into_iter()
            .take(MAX_AWAITED_PER_COMPOSITE)
            .filter(|dependency| self.file(cid, dependency))
            .collect();
        self.hold(block, indexed);
    }

    /// Park a composite the ACP gate refused under its document's `object`
    /// and the `refused` composite, re-driven when either changes. Its
    /// document is what releases it, so it is indexed only when filed there:
    /// past that document's bound, or the node's, it is not indexed and takes
    /// no slot, and the replication layer tries it again on its own retry
    /// clock, as every retryable skip. Filed under the refused composite
    /// alone, it would hold a slot no change of the document and no sweep
    /// releases; when the refused composite is the parked one, only its own
    /// merge would.
    pub(crate) fn park(&self, block: MergeBlock, object: WaitKey, refused: Cid) {
        let cid = block.cid;
        self.take_entry(&cid);
        if !self.reserve() {
            return;
        }
        if !self.file(cid, &object) {
            self.held.fetch_sub(1, Ordering::AcqRel);
            return;
        }
        let refused = WaitKey::Composite(refused);
        let behind = self.file(cid, &refused).then_some(refused);
        self.hold(block, std::iter::once(object).chain(behind).collect());
    }

    /// Reserve a slot before filing anything, so the bound holds under
    /// concurrent defers. False when the index is full.
    fn reserve(&self) -> bool {
        if self.held.fetch_add(1, Ordering::AcqRel) >= self.capacity() {
            self.held.fetch_sub(1, Ordering::AcqRel);
            return false;
        }
        true
    }

    /// File `cid` under `dependency` if the key has room and does not hold
    /// it already. Returns whether it was filed.
    fn file(&self, cid: Cid, dependency: &WaitKey) -> bool {
        let waiters = self.waiters.get_or_insert(
            dependency.clone(),
            Arc::new(CidSet::with_hasher(RandomState::default())),
        );
        waiters.len() < MAX_WAITERS_PER_DEPENDENCY && waiters.insert_if_absent(cid, ()).is_none()
    }

    /// Record the entry of a composite filed under `indexed`, in the slot
    /// [`Self::reserve`] took; filed under nothing, the slot is released.
    fn hold(&self, block: MergeBlock, indexed: Vec<WaitKey>) {
        let cid = block.cid;
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

    /// Release every composite parked under up to `objects` ACP objects, as
    /// a change of their relationships would: the bounded sweep's entry for
    /// refusals nothing on this node releases, such as a grant another node
    /// made. Returns how many objects were released.
    pub(crate) fn release_parked(&self, objects: usize) -> usize {
        let keys: Vec<WaitKey> = self
            .waiters
            .keys()
            .filter(|key| matches!(key, WaitKey::AcpObject { .. }))
            .take(objects)
            .collect();
        let released = keys.len();
        self.release(keys);
        released
    }

    /// Release every composite parked under an object of `policy`, as a
    /// change of each object's relationships would: the home that owns the
    /// policy has a new state, and any of its objects may have changed.
    /// The policy matches however its hex identifier is spelled. Returns
    /// how many objects were released.
    pub(crate) fn release_policy(&self, policy: &str) -> usize {
        let keys = self.parked_under(policy);
        let released = keys.len();
        self.release(keys);
        released
    }

    /// Forget every composite parked under an object of `policy`, which no
    /// longer has a home whose state could let it in. It stays unmerged, so
    /// the replication layer tries it again on its own retry clock, as it
    /// does every change this index does not hold. Returns how many
    /// composites were forgotten.
    pub(crate) fn forget_policy(&self, policy: &str) -> usize {
        let mut forgotten = 0;
        for key in self.parked_under(policy) {
            let Some(waiters) = self.waiters.get(&key) else {
                continue;
            };
            for (waiter, ()) in waiters.iter() {
                if self.take_entry(&waiter).is_some() {
                    forgotten += 1;
                }
            }
            self.waiters.remove(&key);
        }
        forgotten
    }

    /// How many objects of `policy` hold a parked composite.
    pub(crate) fn objects_parked_under(&self, policy: &str) -> usize {
        self.parked_under(policy).len()
    }

    /// The ACP objects of `policy` something is parked under.
    fn parked_under(&self, policy: &str) -> Vec<WaitKey> {
        self.waiters
            .keys()
            .filter(|key| {
                matches!(key, WaitKey::AcpObject { policy: parked, .. } if same_policy(parked, policy))
            })
            .collect()
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

    /// Drop a composite's own entry, if it has one: it merged, so nothing it
    /// awaited can release it any more, and its slot would otherwise be held
    /// for the life of the process.
    pub(crate) fn forget(&self, cid: &Cid) -> bool {
        self.take_entry(cid).is_some()
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

/// Whether two spellings name one policy: policy identifiers are hex, which
/// a caller may write with a `0x` prefix and in either case.
fn same_policy(a: &str, b: &str) -> bool {
    fn bare(id: &str) -> &str {
        id.strip_prefix("0x")
            .or_else(|| id.strip_prefix("0X"))
            .unwrap_or(id)
    }
    bare(a).eq_ignore_ascii_case(bare(b))
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

    /// A new state of a policy's home releases what is parked under that
    /// policy's objects, and nothing parked under another policy or
    /// deferred by governance.
    #[test]
    fn a_policys_new_state_releases_only_what_is_parked_under_it() {
        let index = DeferredMerges::default();
        index.park(block(1), WaitKey::acp_object("p", "r", "doc-1"), cid(1));
        index.park(block(2), WaitKey::acp_object("p", "r", "doc-2"), cid(2));
        index.park(block(3), WaitKey::acp_object("q", "r", "doc-3"), cid(3));
        index.defer(block(4), vec![key(4)]);
        assert_eq!(index.release_policy("p"), 2);
        assert_eq!(index.release_policy("p"), 0);
        let mut ready = Vec::new();
        while let Some(block) = index.take_ready() {
            ready.push(block.cid);
        }
        ready.sort();
        let mut expected = vec![cid(1), cid(2)];
        expected.sort();
        assert_eq!(ready, expected);
        assert_eq!(index.len(), 2);
    }

    /// A policy's new state releases what is parked under it however
    /// either side spells its identifier.
    #[test]
    fn a_policys_new_state_releases_it_under_any_spelling() {
        let index = DeferredMerges::default();
        index.park(
            block(1),
            WaitKey::acp_object("0xABCD", "r", "doc-1"),
            cid(1),
        );
        index.park(block(2), WaitKey::acp_object("abcd", "r", "doc-2"), cid(2));
        assert_eq!(index.objects_parked_under("abcd"), 2);
        assert_eq!(index.release_policy("0Xabcd"), 2);
        assert_eq!(index.objects_parked_under("ABCD"), 0);
    }

    /// Forgetting a policy drops what is parked under it, frees its slots
    /// and releases nothing; what waits under another policy stays.
    #[test]
    fn forgetting_a_policy_drops_only_what_is_parked_under_it() {
        let index = DeferredMerges::default();
        index.park(block(1), WaitKey::acp_object("p", "r", "doc-1"), cid(1));
        index.park(block(2), WaitKey::acp_object("P", "r", "doc-2"), cid(2));
        index.park(block(3), WaitKey::acp_object("q", "r", "doc-3"), cid(3));
        assert_eq!(index.forget_policy("p"), 2);
        assert_eq!(index.len(), 1);
        assert!(!index.has_ready());
        assert_eq!(index.objects_parked_under("p"), 0);
        assert_eq!(index.release_policy("q"), 1);
    }

    /// The sweep releases what is parked under ACP objects, and nothing
    /// that awaits a composite.
    #[test]
    fn the_sweep_releases_parked_composites_only() {
        let index = DeferredMerges::default();
        index.park(block(1), WaitKey::acp_object("p", "r", "doc-1"), cid(1));
        index.park(block(2), WaitKey::acp_object("p", "r", "doc-2"), cid(2));
        index.defer(block(3), vec![key(3)]);
        assert_eq!(index.release_parked(1), 1);
        assert_eq!(index.release_parked(8), 1);
        assert_eq!(index.release_parked(8), 0);
        let mut ready = Vec::new();
        while let Some(block) = index.take_ready() {
            ready.push(block.cid);
        }
        ready.sort();
        let mut expected = vec![cid(1), cid(2)];
        expected.sort();
        assert_eq!(ready, expected);
        assert_eq!(index.len(), 1);
    }

    /// Past its document's bound, a parked composite is not indexed: it takes
    /// no slot and sits under no key, its own CID included, so nothing that
    /// can never release it holds it.
    #[test]
    fn a_change_parked_past_its_documents_bound_is_not_indexed() {
        let index = DeferredMerges::default();
        let document = WaitKey::acp_object("p", "r", "doc-1");
        for n in 0..MAX_WAITERS_PER_DEPENDENCY as u64 {
            index.park(block(n), document.clone(), cid(n));
        }
        assert_eq!(index.len(), MAX_WAITERS_PER_DEPENDENCY);
        let past = MAX_WAITERS_PER_DEPENDENCY as u64;
        index.park(block(past), document.clone(), cid(past));
        assert_eq!(index.len(), MAX_WAITERS_PER_DEPENDENCY);
        assert!(!index.forget(&cid(past)));
        assert!(index.waiters.get(&WaitKey::Composite(cid(past))).is_none());
        assert_eq!(index.held.load(Ordering::Acquire), index.len());

        // A parked change with room is filed under its document and the
        // refused composite, and either releases it.
        let refused = cid(past + 100);
        index.park(
            block(past + 1),
            WaitKey::acp_object("p", "r", "doc-2"),
            refused,
        );
        assert_eq!(index.len(), MAX_WAITERS_PER_DEPENDENCY + 1);
        index.release([WaitKey::Composite(refused)]);
        assert_eq!(
            index.take_ready().map(|block| block.cid),
            Some(cid(past + 1))
        );
        assert!(index
            .waiters
            .get(&WaitKey::acp_object("p", "r", "doc-2"))
            .is_none());
        assert_eq!(index.held.load(Ordering::Acquire), index.len());
    }

    /// A composite that merged while its own entry was being filed would
    /// otherwise hold a slot forever: nothing it awaited can release it.
    #[test]
    fn a_merged_composite_forgets_its_own_entry() {
        let index = DeferredMerges::default();
        index.defer(block(1), vec![key(1)]);
        assert_eq!(index.len(), 1);
        assert!(index.forget(&cid(1)));
        assert_eq!(index.len(), 0);
        assert!(!index.has_waiters());
        assert_eq!(index.held.load(Ordering::Acquire), 0);
        assert!(!index.forget(&cid(1)));
    }
}
