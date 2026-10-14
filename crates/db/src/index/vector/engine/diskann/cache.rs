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

use crate::index::vector::store::NodeId;

pub type Codes = HopscotchMap<NodeId, Vec<u8>, RandomState>;

/// A reload swaps the whole [`Loaded`] in one step, and misses fill the
/// current map in place. A fill that races a reload can land in the map the
/// reload just replaced; that code is read again on its next miss.
#[derive(Debug, Default)]
pub struct CodeCache {
    loaded: Atom<Arc<Loaded>>,
}

#[derive(Debug, Default)]
struct Loaded {
    /// The graph generation the codes were loaded at; `None` until loaded.
    generation: Option<u64>,
    codes: Codes,
}

impl CodeCache {
    pub fn get(&self, id: NodeId) -> Option<Vec<u8>> {
        self.loaded.load().codes.get(&id)
    }

    pub fn insert(&self, id: NodeId, code: Vec<u8>) {
        self.loaded.load().codes.insert(id, code);
    }

    /// Whether the codes are at least as new as `generation`.
    pub fn is_current(&self, generation: u64) -> bool {
        self.loaded
            .load()
            .generation
            .is_some_and(|loaded| loaded >= generation)
    }

    /// Replaces every code, unless a concurrent reload already got further.
    pub fn replace(&self, generation: u64, codes: Codes) {
        let fresh = Arc::new(Loaded {
            generation: Some(generation),
            codes,
        });
        self.loaded.rcu(|current| {
            if current.generation.is_none_or(|loaded| loaded < generation) {
                fresh.clone()
            } else {
                current.clone()
            }
        });
    }
}

/// One [`CodeCache`] per resident-codes index, owned by the database.
#[derive(Debug, Default)]
pub struct CodeCaches {
    by_index: HopscotchMap<(u32, u32), Arc<CodeCache>, RandomState>,
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
