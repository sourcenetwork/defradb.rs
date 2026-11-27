//! PQ codes of a resident-codes index, kept in memory and shared by every
//! search on the node, beside the copy of its pending list that every DiskANN
//! index keeps ([`PendingCache`]).
//!
//! Codes only order the walk; the final ranking reads full vectors inside the
//! query's transaction. So a stale or missing code can cost recall but never
//! correctness, and the cache needs no transactional bookkeeping: it reloads
//! when a merge has run since it was loaded, and fills misses as it goes.
//!
//! What it holds is charged to the node's [`MemoryBudget`]. A load takes
//! codes until the budget has no more room, and a miss is kept only where
//! there is room for it; a code left out is read from storage each time. A
//! reload drops the old codes first, so they never squeeze the new ones out
//! (`proofs/tla/VectorMemoryBudget.tla`), and only one search reloads each
//! generation: the others read codes from storage until it is done.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use kovan::Atom;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use rapidhash::RapidHashMap;

use super::pending_cache::PendingCache;
use super::reload::Reload;
use crate::index::error::{Error, Result};
use crate::index::vector::budget::{Charge, MemoryBudget};
use crate::index::vector::quantize::ProductQuantizer;
use crate::index::vector::store::NodeId;

/// Codes loaded in one pass, back to back in one array with an index of where
/// each starts. A concurrent map holding each code as its own allocation cost
/// about six times the codes themselves at 1M vectors.
#[derive(Debug, Default)]
pub struct Codes {
    index: RapidHashMap<NodeId, u32>,
    bytes: Vec<u8>,
    len: usize,
}

impl Codes {
    /// Every code of one index has the same length, set by the first; a code
    /// of another length would shift every later slot, so it is refused.
    pub fn insert(&mut self, id: NodeId, code: &[u8]) -> Result<()> {
        if self.index.is_empty() {
            self.len = code.len();
        }
        if code.len() != self.len {
            return Err(Error::DiskAnnCodeLength {
                id: id.0,
                expected: self.len,
                got: code.len(),
            });
        }
        let slot = u32::try_from(self.bytes.len() / self.len.max(1))
            .map_err(|_| Error::DiskAnnCodeSlots { id: id.0 })?;
        self.index.insert(id, slot);
        self.bytes.extend_from_slice(code);
        Ok(())
    }

    fn get(&self, id: NodeId) -> Option<&[u8]> {
        let slot = *self.index.get(&id)? as usize;
        Some(&self.bytes[slot * self.len..][..self.len])
    }

    pub(super) fn shrink_to_fit(&mut self) {
        self.index.shrink_to_fit();
        self.bytes.shrink_to_fit();
    }

    /// The bytes the codes and their index occupy as allocated.
    pub fn resident_bytes(&self) -> u64 {
        let slots = table_slots(self.index.capacity());
        (self.bytes.capacity() + slots * CODE_SLOT_BYTES) as u64
    }

    /// At least what [`resident_bytes`](Self::resident_bytes) is once a
    /// `code_len`-byte code is added and the codes are shrunk to fit: the
    /// index rounds its slots up to a power of two, so each code counts
    /// two slots.
    pub fn bound_with_one_more(&self, code_len: usize) -> u64 {
        let codes = self.index.len() as u64 + 1;
        codes * (code_len + 2 * CODE_SLOT_BYTES) as u64 + MIN_TABLE_BYTES
    }
}

/// A slot of the code index: its key, its slot number and a control byte.
const CODE_SLOT_BYTES: usize = size_of::<(NodeId, u32)>() + 1;

/// The smallest index table the map allocates, so a handful of codes is
/// never charged less than it holds.
const MIN_TABLE_BYTES: u64 = (8 * CODE_SLOT_BYTES) as u64;

/// What a miss costs besides its code: the key, the code's vector and a
/// slot in the concurrent map.
const MISS_OVERHEAD: u64 = (size_of::<NodeId>() + size_of::<Vec<u8>>() + 16) as u64;

/// The slots behind a hash table with room for `capacity` entries, which it
/// keeps at most seven eighths full.
fn table_slots(capacity: usize) -> usize {
    match capacity {
        0 => 0,
        1..7 => capacity + 1,
        _ => (capacity * 8).div_ceil(7),
    }
}

/// A reload swaps the whole `Loaded` in one step, and misses fill a small
/// map beside the loaded codes. A fill that races a reload can land in the
/// map the reload just replaced; that code is read again on its next miss.
#[derive(Debug)]
pub struct CodeCache {
    pub(super) loaded: Atom<Arc<Loaded>>,
    pub pending: PendingCache,
    quantizers: Atom<SharedQuantizers>,
    budget: Arc<MemoryBudget>,
    /// The next reload's ticket.
    reloads: AtomicU64,
}

/// A cache nothing is refused from, for an index used outside a node.
impl Default for CodeCache {
    fn default() -> Self {
        Self::with_budget(MemoryBudget::unlimited())
    }
}

/// The quantizers of the last `SHARED_TRAININGS` trainings shared, newest
/// first.
type SharedQuantizers = [Option<SharedQuantizer>; SHARED_TRAININGS];

/// Two, so writers on each side of a rebuild (old snapshots and new) both
/// find theirs instead of evicting each other's on every transaction.
// ponytail: a third training live at once still rebuilds per transaction;
// widen if a rebuild can start before the previous one's writers drain.
const SHARED_TRAININGS: usize = 2;

/// A quantizer and the training whose codebooks it was built from.
#[derive(Debug, Clone)]
struct SharedQuantizer {
    training: u128,
    quantizer: Arc<ProductQuantizer>,
}

/// A reload's claim on a cache: the generation it loads, and a ticket no
/// other reload has, so a reload that was taken over never mistakes a later
/// reload of its generation for itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Claim {
    pub(super) generation: u64,
    pub(super) ticket: u64,
}

#[derive(Debug)]
pub(super) struct Loaded {
    /// The graph generation the codes were loaded at; `None` until loaded.
    pub(super) generation: Option<u64>,
    /// The reload running, if any: the generation it loads and its ticket.
    /// The codes were dropped for it, and misses are not kept, so the reload
    /// has the room they held.
    pub(super) loading: Option<Claim>,
    codes: Codes,
    misses: HopscotchMap<NodeId, Vec<u8>, RandomState>,
    /// What `codes` holds.
    codes_charge: Charge,
    /// What `misses` holds.
    misses_charge: Charge,
}

impl Loaded {
    pub(super) fn empty(
        budget: &Arc<MemoryBudget>,
        generation: Option<u64>,
        loading: Option<Claim>,
    ) -> Self {
        Self {
            generation,
            loading,
            codes: Codes::default(),
            misses: HopscotchMap::default(),
            codes_charge: Charge::empty(budget),
            misses_charge: Charge::empty(budget),
        }
    }

    /// Gives back what this copy holds now that it is no longer current,
    /// rather than when the last search reading it lets go.
    pub(super) fn release(&self) {
        self.codes_charge.release();
        self.misses_charge.release();
    }
}

impl CodeCache {
    /// A cache whose codes and pending copy are charged to `budget`.
    pub fn with_budget(budget: Arc<MemoryBudget>) -> Self {
        Self {
            loaded: Atom::new(Arc::new(Loaded::empty(&budget, None, None))),
            pending: PendingCache::with_budget(budget.clone()),
            quantizers: Atom::default(),
            budget,
            reloads: AtomicU64::new(0),
        }
    }

    pub fn budget(&self) -> &Arc<MemoryBudget> {
        &self.budget
    }

    /// A ticket no reload of this cache has had.
    pub(super) fn next_ticket(&self) -> u64 {
        self.reloads.fetch_add(1, Ordering::Relaxed)
    }

    /// The quantizer built from `training`'s codebooks, if one was shared.
    ///
    /// Codebooks are written once, by the training that names them, so a
    /// match is the stored codebooks whatever snapshot the caller reads.
    pub fn quantizer(&self, training: u128) -> Option<Arc<ProductQuantizer>> {
        self.quantizers
            .load()
            .iter()
            .flatten()
            .find(|shared| shared.training == training)
            .map(|shared| shared.quantizer.clone())
    }

    /// Shares `quantizer`, built from `training`'s codebooks, as the newest.
    /// A training already shared keeps the quantizer it has; otherwise the
    /// oldest of the `SHARED_TRAININGS` is dropped to make room.
    pub fn share_quantizer(&self, training: u128, quantizer: Arc<ProductQuantizer>) {
        let fresh = SharedQuantizer {
            training,
            quantizer,
        };
        self.quantizers.rcu(|current| {
            if current
                .iter()
                .flatten()
                .any(|shared| shared.training == training)
            {
                return current.clone();
            }
            std::array::from_fn(|slot| match slot {
                0 => Some(fresh.clone()),
                older => current[older - 1].clone(),
            })
        });
    }

    pub fn get(&self, id: NodeId) -> Option<Vec<u8>> {
        let loaded = self.loaded.load();
        match loaded.codes.get(id) {
            Some(code) => Some(code.to_vec()),
            None => loaded.misses.get(&id),
        }
    }

    /// Keeps a code read on a miss, where the budget has room for it and no
    /// reload is waiting for that room.
    pub fn insert(&self, id: NodeId, code: Vec<u8>) {
        let loaded = self.loaded.load();
        if loaded.loading.is_some() || loaded.misses.contains_key(&id) {
            return;
        }
        let bytes = code.len() as u64 + MISS_OVERHEAD;
        if loaded.misses_charge.grow(bytes) && loaded.misses.insert_if_absent(id, code).is_some() {
            loaded.misses_charge.shrink(bytes);
        }
    }

    /// Starts reloading the codes for `generation`, unless they are already
    /// that new or another search is reloading them. The old codes are
    /// dropped first, so their room goes to the new ones.
    pub fn begin_reload(&self, generation: u64) -> Option<Reload<'_>> {
        Reload::begin(self, generation)
    }

    /// Publishes a pending-list copy unless a code reload is running, so the
    /// pending copy cannot take the room that reload reserved.
    pub fn publish_pending(&self, next: Arc<super::pending_cache::Copy>) {
        self.pending
            .publish_if(next, || self.loaded.load().loading.is_none());
    }

    /// Whether the codes are at least as new as `generation`.
    pub fn is_current(&self, generation: u64) -> bool {
        self.loaded
            .load()
            .generation
            .is_some_and(|loaded| loaded >= generation)
    }

    /// The bytes the loaded codes and the misses hold.
    pub fn resident_bytes(&self) -> u64 {
        let loaded = self.loaded.load();
        loaded.codes_charge.bytes() + loaded.misses_charge.bytes()
    }

    /// Replaces every code, unless a concurrent reload already got further.
    /// Codes the budget has no room for are not kept.
    pub fn replace(&self, generation: u64, codes: Codes) {
        self.replace_charged(generation, codes, Charge::empty(&self.budget), None);
    }

    /// [`replace`](Self::replace) with the room `charge` already took for
    /// the codes, settled to what they hold once shrunk to fit. A reload's
    /// replace names its `claim`, and replaces only while the reload still
    /// holds it: in the same step, so a reload that took it over never has
    /// its claim wiped by an older one's codes.
    pub(super) fn replace_charged(
        &self,
        generation: u64,
        mut codes: Codes,
        mut charge: Charge,
        claim: Option<Claim>,
    ) {
        codes.shrink_to_fit();
        if !charge.settle(codes.resident_bytes()) {
            charge.release();
            codes = Codes::default();
        }
        let mut fresh = Loaded::empty(&self.budget, Some(generation), None);
        fresh.codes = codes;
        fresh.codes_charge = charge;
        let fresh = Arc::new(fresh);
        let mut replaced = None;
        self.loaded.rcu(|current| {
            replaced = None;
            let newer = current.generation.is_none_or(|loaded| loaded < generation);
            if newer && (claim.is_none() || current.loading == claim) {
                replaced = Some(current.clone());
                fresh.clone()
            } else {
                current.clone()
            }
        });
        match replaced {
            Some(replaced) => replaced.release(),
            None => fresh.release(),
        }
        // The replaced codes are retired to this thread; reclaim them now
        // rather than holding two copies until the thread retires more.
        kovan::flush();
    }
}

/// One [`CodeCache`] per DiskANN index, owned by the database, all charged
/// to one budget.
#[derive(Debug)]
pub struct CodeCaches {
    by_index: HopscotchMap<(u32, u32), Arc<CodeCache>, RandomState>,
    budget: Arc<MemoryBudget>,
}

/// Caches nothing is refused from, for indexes used outside a node.
impl Default for CodeCaches {
    fn default() -> Self {
        Self::new(MemoryBudget::unlimited())
    }
}

#[cfg(test)]
thread_local! {
    /// Test-only: the [`CodeCaches`] this thread built.
    pub(crate) static BUILT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl CodeCaches {
    pub fn new(budget: Arc<MemoryBudget>) -> Self {
        #[cfg(test)]
        BUILT.with(|built| built.set(built.get() + 1));
        Self {
            by_index: HopscotchMap::with_hasher(RandomState::default()),
            budget,
        }
    }

    /// The budget every cache here is charged to.
    pub fn budget(&self) -> &Arc<MemoryBudget> {
        &self.budget
    }

    pub fn get(&self, collection_short_id: u32, index_id: u32) -> Arc<CodeCache> {
        let key = (collection_short_id, index_id);
        // Looked up first so a hit allocates no cache to throw away.
        self.by_index.get(&key).unwrap_or_else(|| {
            let cache = Arc::new(CodeCache::with_budget(self.budget.clone()));
            self.by_index.get_or_insert(key, cache)
        })
    }

    /// Drops the caches of an index that no longer exists. Nothing rebuilds
    /// them, since no search can name a dropped index; a search that began
    /// before the drop and loads a copy afterwards leaves that copy behind.
    pub fn evict(&self, collection_short_id: u32, index_id: u32) {
        self.by_index.remove(&(collection_short_id, index_id));
    }

    /// [`evict`](Self::evict) for every index of a dropped collection.
    pub fn evict_collection(&self, collection_short_id: u32) {
        let dropped: Vec<_> = self
            .by_index
            .keys()
            .filter(|(collection, _)| *collection == collection_short_id)
            .collect();
        for key in dropped {
            self.by_index.remove(&key);
        }
    }

    /// How many indexes have caches.
    pub fn len(&self) -> usize {
        self.by_index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
