//! PQ codes of a resident-codes index, kept in memory and shared by every
//! search on the node.
//!
//! Codes only order the walk; the final ranking reads full vectors inside the
//! query's transaction. So a stale or missing code can cost recall but never
//! correctness, and the cache needs no transactional bookkeeping: it reloads
//! when a merge has run since it was loaded, and fills misses as it goes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use crate::index::vector::store::NodeId;

#[derive(Debug, Default)]
pub struct CodeCache {
    inner: RwLock<Loaded>,
}

#[derive(Debug, Default)]
struct Loaded {
    /// The graph generation the codes were loaded at; `None` until loaded.
    generation: Option<u64>,
    codes: HashMap<NodeId, Vec<u8>>,
}

impl CodeCache {
    pub fn get(&self, id: NodeId) -> Option<Vec<u8>> {
        self.inner.read().unwrap().codes.get(&id).cloned()
    }

    pub fn insert(&self, id: NodeId, code: Vec<u8>) {
        self.inner.write().unwrap().codes.insert(id, code);
    }

    /// Whether the codes are at least as new as `generation`.
    pub fn is_current(&self, generation: u64) -> bool {
        self.inner
            .read()
            .unwrap()
            .generation
            .is_some_and(|loaded| loaded >= generation)
    }

    /// Replaces every code, unless a concurrent reload already got further.
    pub fn replace(&self, generation: u64, codes: HashMap<NodeId, Vec<u8>>) {
        let mut inner = self.inner.write().unwrap();
        if inner.generation.is_none_or(|loaded| loaded < generation) {
            *inner = Loaded {
                generation: Some(generation),
                codes,
            };
        }
    }
}

/// One [`CodeCache`] per resident-codes index, owned by the database.
#[derive(Debug, Default)]
pub struct CodeCaches {
    by_index: Mutex<HashMap<(u32, u32), Arc<CodeCache>>>,
}

impl CodeCaches {
    pub fn get(&self, collection_short_id: u32, index_id: u32) -> Arc<CodeCache> {
        self.by_index
            .lock()
            .unwrap()
            .entry((collection_short_id, index_id))
            .or_default()
            .clone()
    }
}
