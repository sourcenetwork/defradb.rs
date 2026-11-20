//! Prolly tree nodes in regolith, one ContentAddressed key per node
//! (`proofs/tla/ProllyNodeCache.tla`).
//!
//! A node is stored under `t` and the CID of its encoding (raw codec,
//! SHA-256), so regolith classifies every node key as content addressed: a
//! put is blind and never conflicts, and a read is never validated. A node
//! never changes once named, so any copy of one, here or in a
//! [`prolly::Cached`] in front of this store, is never stale.
//!
//! Puts wait in memory, encoded, until they pass `pending_budget` bytes or the
//! tree flushes, and are then written in one `transact`. A read finds a
//! pending node before it looks in regolith, and the pending set is cleared
//! only after its transaction commits, so a node put is always readable.

use std::sync::Arc;

use cid::{multihash::Multihash, Cid};
use prolly::{Hash, Node};
use rapidhash::{HashMapExt, RapidHashMap};

use crate::corekv::{
    transact, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue, RetryOptions, RetryTxn, Store,
};

/// The multicodec of raw bytes.
const RAW: u64 = 0x55;
/// The multihash code of SHA-256.
const SHA2_256: u64 = 0x12;

/// The bytes every node key starts with (`families`).
const PREFIX: u8 = b't';

/// A [`prolly::Store`] over a regolith store.
pub struct ProllyNodes<S> {
    store: Arc<S>,
    retry: RetryOptions,
    pending_budget: usize,
    pending: RapidHashMap<Hash, Vec<u8>>,
    pending_bytes: usize,
}

impl<S> ProllyNodes<S> {
    /// Nodes in `store`, written under `retry` once `pending_budget` bytes of
    /// them wait.
    pub fn new(store: Arc<S>, retry: RetryOptions, pending_budget: usize) -> Self {
        Self {
            store,
            retry,
            pending_budget,
            pending: RapidHashMap::new(),
            pending_bytes: 0,
        }
    }

    /// The encoded bytes of the nodes put and not yet written.
    pub fn pending_bytes(&self) -> usize {
        self.pending_bytes
    }
}

/// The regolith key of the node `hash` names.
pub fn node_key(hash: &Hash) -> Vec<u8> {
    let digest = Multihash::<64>::wrap(SHA2_256, &hash.0).expect("a 32-byte digest fits in 64");
    let mut key = vec![PREFIX];
    key.extend_from_slice(&Cid::new_v1(RAW, digest).to_bytes());
    key
}

fn store_error(error: impl std::fmt::Display) -> prolly::Error {
    prolly::Error::Store(error.to_string())
}

impl<S> prolly::Store for ProllyNodes<S>
where
    S: Store + 'static,
{
    async fn get(&self, hash: &Hash) -> prolly::Result<Option<Arc<Node>>> {
        if let Some(bytes) = self.pending.get(hash) {
            return Node::decode(bytes).map(|node| Some(Arc::new(node)));
        }
        let txn = self.store.new_txn(true).await.map_err(store_error)?;
        let read = txn.get(&node_key(hash)).await;
        txn.discard();
        match read.map_err(store_error)? {
            Some(bytes) => Node::decode(&bytes).map(|node| Some(Arc::new(node))),
            None => Ok(None),
        }
    }

    async fn put(&mut self, node: Node) -> prolly::Result<Hash> {
        let bytes = node.encode();
        let hash = Hash::of(&bytes);
        if !self.pending.contains_key(&hash) {
            self.pending_bytes += bytes.len();
            self.pending.insert(hash, bytes);
        }
        if self.pending_bytes >= self.pending_budget {
            prolly::Store::flush(self).await?;
        }
        Ok(hash)
    }

    async fn flush(&mut self) -> prolly::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let writes = Arc::new(std::mem::take(&mut self.pending));
        let attempt = Box::new(WriteNodes {
            writes: Arc::clone(&writes),
        });
        let written: Result<(), _> = transact(&*self.store, self.retry, None, attempt).await;
        match written {
            Ok(()) => {
                self.pending_bytes = 0;
                Ok(())
            }
            Err(error) => {
                // Not written: the nodes stay pending, still readable, and
                // the next flush writes them.
                self.pending = Arc::unwrap_or_clone(writes);
                Err(prolly::Error::Store(format!("{error:?}")))
            }
        }
    }
}

/// One attempt at writing the pending nodes. It owns them behind an `Arc`,
/// so a re-run after a conflict writes the same bytes again.
struct WriteNodes {
    writes: Arc<RapidHashMap<Hash, Vec<u8>>>,
}

impl RetryAttemptFn for WriteNodes {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            for (hash, bytes) in self.writes.iter() {
                txn.set(&node_key(hash), bytes).await?;
            }
            Ok(Box::new(()) as RetryAttemptValue)
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "prolly_nodes_tests.rs"]
mod tests;
