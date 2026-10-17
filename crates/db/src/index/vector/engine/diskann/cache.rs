//! PQ codes of a resident-codes index, kept in memory and shared by every
//! search on the node.
//!
//! Codes only order the walk; the final ranking reads full vectors inside the
//! query's transaction. So a stale or missing code can cost recall but never
//! correctness, and the cache needs no transactional bookkeeping: it reloads
//! when a merge has run since it was loaded, and fills misses as it goes.

use std::sync::Arc;

use kovan::Atom;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use rapidhash::RapidHashMap;

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
    /// Every code of one index has the same length, set by the first.
    pub fn insert(&mut self, id: NodeId, code: &[u8]) {
        if self.index.is_empty() {
            self.len = code.len();
        }
        debug_assert_eq!(code.len(), self.len);
        self.index
            .insert(id, (self.bytes.len() / self.len.max(1)) as u32);
        self.bytes.extend_from_slice(code);
    }

    fn get(&self, id: NodeId) -> Option<&[u8]> {
        let slot = *self.index.get(&id)? as usize;
        Some(&self.bytes[slot * self.len..][..self.len])
    }

    fn shrink_to_fit(&mut self) {
        self.index.shrink_to_fit();
        self.bytes.shrink_to_fit();
    }
}

/// A reload swaps the whole [`Loaded`] in one step, and misses fill a small
/// map beside the loaded codes. A fill that races a reload can land in the
/// map the reload just replaced; that code is read again on its next miss.
#[derive(Debug, Default)]
pub struct CodeCache {
    loaded: Atom<Arc<Loaded>>,
}

#[derive(Debug, Default)]
struct Loaded {
    /// The graph generation the codes were loaded at; `None` until loaded.
    generation: Option<u64>,
    codes: Codes,
    misses: HopscotchMap<NodeId, Vec<u8>, RandomState>,
}

impl CodeCache {
    pub fn get(&self, id: NodeId) -> Option<Vec<u8>> {
        let loaded = self.loaded.load();
        match loaded.codes.get(id) {
            Some(code) => Some(code.to_vec()),
            None => loaded.misses.get(&id),
        }
    }

    pub fn insert(&self, id: NodeId, code: Vec<u8>) {
        self.loaded.load().misses.insert(id, code);
    }

    /// Whether the codes are at least as new as `generation`.
    pub fn is_current(&self, generation: u64) -> bool {
        self.loaded
            .load()
            .generation
            .is_some_and(|loaded| loaded >= generation)
    }

    /// Replaces every code, unless a concurrent reload already got further.
    pub fn replace(&self, generation: u64, mut codes: Codes) {
        codes.shrink_to_fit();
        let fresh = Arc::new(Loaded {
            generation: Some(generation),
            codes,
            misses: HopscotchMap::default(),
        });
        self.loaded.rcu(|current| {
            if current.generation.is_none_or(|loaded| loaded < generation) {
                fresh.clone()
            } else {
                current.clone()
            }
        });
        // The replaced codes are retired to this thread; reclaim them now
        // rather than holding two copies until the thread retires more.
        kovan::flush();
    }
}

/// One [`CodeCache`] per resident-codes index, owned by the database.
#[derive(Debug)]
pub struct CodeCaches {
    by_index: HopscotchMap<(u32, u32), Arc<CodeCache>, RandomState>,
}

impl Default for CodeCaches {
    fn default() -> Self {
        #[cfg(test)]
        BUILT.with(|built| built.set(built.get() + 1));
        Self {
            by_index: HopscotchMap::with_hasher(RandomState::default()),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Test-only: the [`CodeCaches`] this thread built.
    pub(crate) static BUILT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl CodeCaches {
    pub fn get(&self, collection_short_id: u32, index_id: u32) -> Arc<CodeCache> {
        let key = (collection_short_id, index_id);
        // Looked up first so a hit allocates no cache to throw away.
        self.by_index
            .get(&key)
            .unwrap_or_else(|| self.by_index.get_or_insert(key, Arc::default()))
    }
}
