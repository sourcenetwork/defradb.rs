//! Where nodes live, by hash.

use std::{future::Future, sync::Arc};

use rapidhash::{HashMapExt, RapidHashMap};

use crate::{error::Result, hash::Hash, node::Node};

/// A content-addressed node store.
///
/// Readers trust what `get` returns: a node whose `hash()` is the hash asked
/// for, one level below the parent that named it, with its items sorted by key.
/// `MemStore` holds only nodes it hashed itself. A store fed by another peer
/// must check this before returning a node.
///
/// A node is named by the hash of its bytes, so the node under a hash never
/// changes: a copy kept anywhere is never stale.
///
/// A tree operation only puts nodes; it never makes them durable itself. A
/// store that persists them commits them in the transaction that publishes
/// the root naming them (`ProllyNodes::publish` in the storage crate), so no
/// node is stored apart from a root, and no root apart from its nodes.
///
/// The futures need not be `Send`: a tree is read and written on one worker.
pub trait Store {
    /// The node stored under `hash`, if any. A node `put` and not yet
    /// committed is found too.
    fn get(&self, hash: &Hash) -> impl Future<Output = Result<Option<Arc<Node>>>>;
    /// Stores `node` under its hash and returns the hash.
    fn put(&mut self, node: Node) -> impl Future<Output = Result<Hash>>;
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
    async fn get(&self, hash: &Hash) -> Result<Option<Arc<Node>>> {
        Ok(self.nodes.get(hash).cloned())
    }

    async fn put(&mut self, node: Node) -> Result<Hash> {
        let hash = node.hash();
        self.nodes.entry(hash).or_insert_with(|| Arc::new(node));
        Ok(hash)
    }
}
