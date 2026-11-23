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
//!
//! [`ProllyNodes::reclaim`] sweeps one bounded page of the nodes no live root
//! names (`proofs/tla/ProllyNodeReclaim.tla`). Its delete transaction reads
//! the live roots itself, with validated reads, so a root published before it
//! commits re-runs it against that root. `t` is a key family, not a
//! [`Namespace`](crate::namespace::Namespace), so the namespace walks (dump,
//! purge) do not see it either; a purged store has no `t` key, and a restored
//! one is rebuilt from its heads and reclaimed against those roots.

use std::sync::Arc;

use cid::{multihash::Multihash, Cid};
use prolly::{Hash, Node};
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};

use crate::corekv::{
    engine_retry_options, transact, IterOptions, MaybeSendSync, RetryAttemptFn, RetryAttemptFuture,
    RetryAttemptValue, RetryOptions, RetryTxn, Store,
};
use defra_core::thread_bounds::MaybeBoxFuture;

/// The multicodec of raw bytes.
const RAW: u64 = 0x55;
/// The multihash code of SHA-256.
const SHA2_256: u64 = 0x12;

/// The bytes every node key starts with (`families`).
const PREFIX: u8 = b't';

/// How many node keys one bounded sweep examines before it resumes.
pub const NODES_PER_SWEEP: usize = 8192;

/// What one bounded call to [`ProllyNodes::reclaim`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReclaimPage {
    /// Node keys examined on the page.
    pub scanned: usize,
    /// Distinct nodes the live roots named.
    pub marked: usize,
    /// Unmarked node keys deleted.
    pub deleted: usize,
    /// Where the next call resumes; `None` once the scan reached the end of
    /// `t`.
    pub next_start: Option<Vec<u8>>,
}

/// What [`LiveRoots::read`] returns: the root of every live tree.
pub type LiveRootsFuture<'txn> = MaybeBoxFuture<'txn, crate::corekv::Result<Vec<Hash>>>;

/// The roots [`ProllyNodes::reclaim`] keeps, read inside its delete
/// transaction.
pub trait LiveRoots: MaybeSendSync {
    /// Reads every live root through `txn`, with validated reads of the keys
    /// that hold them, so a root published before the delete commits is a
    /// conflict and the delete runs again.
    fn read<'txn>(&'txn self, txn: &'txn mut dyn RetryTxn) -> LiveRootsFuture<'txn>;
}

/// A [`prolly::Store`] over a regolith store.
pub struct ProllyNodes<S> {
    store: Arc<S>,
    retry: RetryOptions,
    pending_budget: usize,
    pending: Arc<RapidHashMap<Hash, Vec<u8>>>,
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
            pending: Arc::new(RapidHashMap::new()),
            pending_bytes: 0,
        }
    }

    /// The encoded bytes of the nodes put and not yet written.
    pub fn pending_bytes(&self) -> usize {
        self.pending_bytes
    }
}

impl<S> ProllyNodes<S>
where
    S: Store + 'static,
{
    /// Sweeps one bounded page of `t`: deletes the page's keys that no root
    /// `roots` reads names. `start` resumes a stopped sweep.
    ///
    /// The roots are read in the delete transaction, so the page deletes
    /// nothing a root published before its commit names. A node put is blind
    /// and conflicts with nothing, so a node committed before its root is
    /// published can be swept in between: a page is safe beside a writer
    /// only when that writer publishes a root in the transaction that writes
    /// the root's nodes. A tree edit flushes its nodes before it returns the
    /// root, so a caller that publishes the root afterwards must not run a
    /// page between an edit and the publication of its root.
    pub async fn reclaim<R>(
        store: &S,
        roots: Arc<R>,
        start: Option<Vec<u8>>,
        limit: usize,
    ) -> prolly::Result<ReclaimPage>
    where
        R: LiveRoots + 'static,
    {
        let (keys, next_start) = node_keys(store, start.as_deref(), limit).await?;
        let scanned = keys.len();
        if keys.is_empty() {
            return Ok(ReclaimPage {
                scanned,
                marked: 0,
                deleted: 0,
                next_start,
            });
        }
        let attempt = Box::new(ReclaimNodes { keys, roots });
        let (marked, deleted) = transact::<_, (u64, u64)>(
            store,
            engine_retry_options(crate::corekv::DEFAULT_MAX_TXN_RETRIES),
            None,
            attempt,
        )
        .await
        .map_err(|error| store_error(error.into_storage()))?;
        let (marked, deleted) = (marked as usize, deleted as usize);
        Ok(ReclaimPage {
            scanned,
            marked,
            deleted,
            next_start,
        })
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

fn node_hash(key: &[u8]) -> Option<Hash> {
    let rest = key.get(1..)?;
    let cid = Cid::try_from(rest).ok()?;
    Hash::from_slice(cid.hash().digest()).ok()
}

async fn node_keys<S>(
    store: &S,
    start: Option<&[u8]>,
    limit: usize,
) -> prolly::Result<(Vec<Vec<u8>>, Option<Vec<u8>>)>
where
    S: Store,
{
    let mut options = IterOptions::new()
        .with_prefix(vec![PREFIX])
        .with_keys_only(true);
    if let Some(start) = start {
        options = options.with_start(start.to_vec());
    }
    let txn = store.new_txn(true).await.map_err(store_error)?;
    let mut scan = txn.iterator(options).await.map_err(store_error)?;
    let mut keys = Vec::with_capacity(limit.min(128));
    let mut page_end = None;
    while keys.len() < limit {
        let Some(pair) = scan.next().await.map_err(store_error)? else {
            page_end = None;
            break;
        };
        if node_hash(&pair.key).is_some() {
            keys.push(pair.key.to_vec());
        }
        let mut raw_key = pair.key.to_vec();
        raw_key.push(0);
        page_end = Some(raw_key);
    }
    scan.close().await.map_err(store_error)?;
    txn.discard();
    Ok((keys, page_end))
}

async fn mark_roots(
    txn: &mut dyn RetryTxn,
    roots: Vec<Hash>,
) -> crate::corekv::Result<RapidHashSet<Hash>> {
    let mut marked = RapidHashSet::new();
    let mut pending = roots;
    while let Some(hash) = pending.pop() {
        if !marked.insert(hash) {
            continue;
        }
        let bytes = txn.get(&node_key(&hash)).await?.ok_or_else(|| {
            crate::corekv::Error::Other(prolly::Error::MissingNode(hash).to_string())
        })?;
        let node =
            Node::decode(&bytes).map_err(|error| crate::corekv::Error::Other(error.to_string()))?;
        if node.level == 0 {
            continue;
        }
        for item in node.items {
            pending.push(
                Hash::from_slice(&item.value)
                    .map_err(|error| crate::corekv::Error::Other(error.to_string()))?,
            );
        }
    }
    Ok(marked)
}

/// One attempt at one page. It reads the live roots in its own transaction,
/// so a run that lost to a root's publication marks again from that root.
struct ReclaimNodes<R> {
    keys: Vec<Vec<u8>>,
    roots: Arc<R>,
}

impl<R> RetryAttemptFn for ReclaimNodes<R>
where
    R: LiveRoots,
{
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let roots = self.roots.read(txn).await?;
            let marked = mark_roots(txn, roots).await?;
            let mut deleted = 0;
            for key in &self.keys {
                let Some(hash) = node_hash(key) else {
                    continue;
                };
                if marked.contains(&hash) {
                    continue;
                }
                txn.delete(key).await?;
                deleted += 1;
            }
            Ok(Box::new((marked.len() as u64, deleted as u64)) as RetryAttemptValue)
        })
    }
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
            Arc::make_mut(&mut self.pending).insert(hash, bytes);
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
        // The attempt shares the pending set instead of taking it, so a flush
        // that fails or is dropped before its commit leaves every node pending
        // and readable; only a commit clears them.
        let attempt = Box::new(WriteNodes {
            writes: Arc::clone(&self.pending),
        });
        let written: Result<(), _> = transact(&*self.store, self.retry, None, attempt).await;
        written.map_err(|error| store_error(error.into_storage()))?;
        self.pending = Arc::new(RapidHashMap::new());
        self.pending_bytes = 0;
        Ok(())
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
