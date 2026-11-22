//! Where nodes live, by hash.

use std::sync::Arc;

use rapidhash::{HashMapExt, RapidHashMap};

use crate::{hash::Hash, node::Node};

/// A content-addressed node store.
///
/// Readers trust what `get` returns: a node whose `hash()` is the hash asked
/// for, one level below the parent that named it, with its items sorted by key.
/// `MemStore` holds only nodes it hashed itself. A store fed by another peer
/// must check this before returning a node.
pub trait Store {
    /// The node stored under `hash`, if any.
    fn get(&self, hash: &Hash) -> Option<Arc<Node>>;
    /// Stores `node` under its hash and returns the hash.
    fn put(&mut self, node: Node) -> Hash;
}

/// A store in memory.
#[derive(Default)]
pub struct MemStore {
    nodes: RapidHashMap<Hash, Arc<Node>>,
}

impl MemStore {
    /// An empty store.
    pub fn new() -> Self {
        Self {
            nodes: RapidHashMap::new(),
        }
    }

    /// How many distinct nodes it holds.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether it holds no node.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

impl Store for MemStore {
    fn get(&self, hash: &Hash) -> Option<Arc<Node>> {
        self.nodes.get(hash).cloned()
    }

    fn put(&mut self, node: Node) -> Hash {
        let hash = node.hash();
        self.nodes.entry(hash).or_insert_with(|| Arc::new(node));
        hash
    }
}
