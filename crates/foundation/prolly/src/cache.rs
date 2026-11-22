//! Nodes kept in memory up to a byte budget, in front of a store that holds
//! the whole tree (`proofs/tla/ProllyNodeCache.tla`).
//!
//! A node is named by the hash of its bytes, so the node under a hash never
//! changes: a cached node is never stale, and the cache needs no invalidation,
//! only eviction. It belongs to the one worker that reads the tree, so it is a
//! plain owned value behind a `RefCell`, never shared between threads; no
//! borrow of it is held across an `.await`.

use std::{cell::RefCell, collections::VecDeque, sync::Arc};

use rapidhash::{HashMapExt, RapidHashMap};

use crate::{error::Result, hash::Hash, node::Node, store::Store};

/// What a cache has done, and what it holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Reads answered from memory.
    pub hits: u64,
    /// Reads that went to the store.
    pub misses: u64,
    /// Nodes dropped to stay under the budget.
    pub evictions: u64,
    /// The encoded bytes of the nodes it holds now.
    pub bytes: usize,
    /// The most bytes it has held at once.
    pub peak: usize,
}

/// One cached node: the node, its encoded bytes, and whether it was read
/// since the clock hand last passed it.
struct Slot {
    node: Arc<Node>,
    bytes: usize,
    used: bool,
}

/// The nodes held, the clock's ring of their hashes, and the counts.
#[derive(Default)]
struct Held {
    nodes: RapidHashMap<Hash, Slot>,
    ring: VecDeque<Hash>,
    stats: CacheStats,
}

/// A store whose reads go through a node cache of at most `budget` encoded
/// bytes. A miss is filled from the store; once the cache is over budget it
/// evicts by the clock rule (a node read since the hand last passed it gets
/// one more turn), so the upper levels every walk reads stay in memory. A node
/// larger than the whole budget is read through and never cached.
pub struct Cached<S> {
    store: S,
    budget: usize,
    held: RefCell<Held>,
}

impl<S> Cached<S> {
    /// `store`, read through a cache of at most `budget` bytes.
    pub fn new(store: S, budget: usize) -> Self {
        Self {
            store,
            budget,
            held: RefCell::new(Held {
                nodes: RapidHashMap::new(),
                ..Held::default()
            }),
        }
    }

    /// The counts so far.
    pub fn stats(&self) -> CacheStats {
        self.held.borrow().stats
    }

    /// The store behind the cache.
    pub fn inner(&self) -> &S {
        &self.store
    }

    fn lookup(&self, hash: &Hash) -> Option<Arc<Node>> {
        let mut held = self.held.borrow_mut();
        let hit = held.nodes.get_mut(hash).map(|slot| {
            slot.used = true;
            Arc::clone(&slot.node)
        });
        match hit {
            Some(_) => held.stats.hits += 1,
            None => held.stats.misses += 1,
        }
        hit
    }

    fn insert(&self, hash: Hash, node: &Arc<Node>) {
        let bytes = node.encoded_len();
        let mut held = self.held.borrow_mut();
        if bytes > self.budget || held.nodes.contains_key(&hash) {
            return;
        }
        held.nodes.insert(
            hash,
            Slot {
                node: Arc::clone(node),
                bytes,
                used: false,
            },
        );
        held.ring.push_back(hash);
        held.stats.bytes += bytes;
        // Each turn of the hand clears a `used` mark or evicts, so this ends.
        while held.stats.bytes > self.budget {
            let Some(oldest) = held.ring.pop_front() else {
                break;
            };
            let Some(slot) = held.nodes.get_mut(&oldest) else {
                continue;
            };
            if slot.used {
                slot.used = false;
                held.ring.push_back(oldest);
                continue;
            }
            let freed = slot.bytes;
            held.nodes.remove(&oldest);
            held.stats.bytes -= freed;
            held.stats.evictions += 1;
        }
        held.stats.peak = held.stats.peak.max(held.stats.bytes);
    }
}

impl<S> Store for Cached<S>
where
    S: Store,
{
    async fn get(&self, hash: &Hash) -> Result<Option<Arc<Node>>> {
        if let Some(node) = self.lookup(hash) {
            return Ok(Some(node));
        }
        let node = self.store.get(hash).await?;
        if let Some(node) = &node {
            self.insert(*hash, node);
        }
        Ok(node)
    }

    async fn put(&mut self, node: Node) -> Result<Hash> {
        self.store.put(node).await
    }

    async fn flush(&mut self) -> Result<()> {
        self.store.flush().await
    }
}
