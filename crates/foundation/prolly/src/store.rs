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
/// The futures need not be `Send`: a tree is read and written on one worker.
pub trait Store {
    /// The node stored under `hash`, if any. A node `put` and not yet
    /// `flush`ed is found too.
    fn get(&self, hash: &Hash) -> impl Future<Output = Result<Option<Arc<Node>>>>;
    /// Stores `node` under its hash and returns the hash. It is durable once
    /// `flush` returns.
    fn put(&mut self, node: Node) -> impl Future<Output = Result<Hash>>;
    /// Makes every node put so far durable. Every tree operation that writes
    /// calls it before it returns a root, so a root names only stored nodes.
    fn flush(&mut self) -> impl Future<Output = Result<()>>;
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

    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
