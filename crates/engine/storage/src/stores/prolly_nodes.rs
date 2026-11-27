//! Prolly tree nodes in regolith, one ContentAddressed key per node
//! (`proofs/tla/ProllyNodeCache.tla`).
//!
//! A node is stored under `t` and the CID of its encoding (raw codec,
//! SHA-256), so regolith classifies every node key as content addressed: a
//! put is blind and never conflicts, and a read is never validated. A node
//! never changes once named, so any copy of one, here or in a
//! [`prolly::Cached`] in front of this store, is never stale.
//!
//! Puts wait in memory, encoded, until [`ProllyNodes::publish`] writes them in
//! the transaction that publishes the root naming them, so a node is never
//! stored apart from a root. The pending set holds what one publish commits:
//! a few nodes per level for an edit, every node of the tree for a build. A
//! read finds a pending node before it looks in regolith, and the pending set
//! is cleared only after that transaction commits, so a node put is always
//! readable. A build's pending set is held in memory and written in one
//! transaction, so it must stay below both the process's memory budget and
//! regolith's 1 GiB record limit. No production tree uses this store yet;
//! before the first production-sized build, it needs a size cap or a spill
//! under a provisional root tracked by #3279.
//!
//! [`ProllyNodes::reclaim`] sweeps one bounded page of the nodes no live root
//! names (`proofs/tla/ProllyNodeReclaim.tla`), and [`reclaim_every`] runs one
//! page per period. No production owner starts that schedule yet; it starts
//! with the first real [`LiveRoots`] implementation. Its delete transaction
//! reads the live roots itself, with
//! validated reads, so a root published before it commits re-runs it against
//! that root. `t` is a key family, not a
//! [`Namespace`](crate::namespace::Namespace), so the namespace walks (dump,
//! purge) do not see it either; a purged store has no `t` key, and a restored
//! one is rebuilt from its heads and reclaimed against those roots.

use std::any::Any;
use std::sync::Arc;

use cid::{multihash::Multihash, Cid};
use prolly::{Hash, Node};
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};

use crate::corekv::{
    engine_retry_options, transact, IterOptions, MaybeSendSync, RetryAttempt, RetryAttemptError,
    RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue, RetryOptions, RetryTxn, Store,
};
use defra_core::thread_bounds::MaybeBoxFuture;

mod schedule;

pub use schedule::reclaim_every;

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
    pending: Arc<RapidHashMap<Hash, Vec<u8>>>,
    pending_bytes: usize,
}

impl<S> ProllyNodes<S> {
    /// Nodes in `store`, written with their root under `retry`.
    pub fn new(store: Arc<S>, retry: RetryOptions) -> Self {
        Self {
            store,
            retry,
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
    /// Writes every node put since the last publish, and runs `root`, which
    /// publishes the root that names them, in the same transaction. Nodes and
    /// root commit together, so a reclaim page that reads the live roots
    /// either sees the root or conflicts with it. A conflict re-runs both;
    /// the value is that of the run of `root` that committed. Only a commit
    /// clears the pending nodes, so a publish that fails or is dropped leaves
    /// them readable for the next one.
    ///
    /// A build's pending set must fit in memory and its nodes must encode to
    /// less than regolith's 1 GiB record limit. No production tree is built
    /// through this store yet; the first production owner must cap the tree or
    /// spill its nodes under a provisional root before that size can be reached.
    pub async fn publish<T>(&mut self, root: RetryAttempt) -> Result<T, RetryAttemptError>
    where
        T: Any,
    {
        // The attempt shares the pending set instead of taking it, so a
        // publish that fails or is dropped before its commit leaves every
        // node pending and readable; only a commit clears them.
        let attempt = Box::new(PublishNodes {
            writes: Arc::clone(&self.pending),
            root,
        });
        let published = transact(&*self.store, self.retry, None, attempt).await?;
        self.pending = Arc::new(RapidHashMap::new());
        self.pending_bytes = 0;
        Ok(published)
    }

    /// Sweeps one bounded page of `t`: deletes the page's keys that no root
    /// `roots` reads names. `start` resumes a stopped sweep.
    ///
    /// The roots are read in the delete transaction, so the page deletes
    /// nothing a root published before its commit names. A root's nodes
    /// commit in the transaction that publishes it ([`publish`](Self::publish)),
    /// so no stored node waits for a root the page cannot see yet.
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
        Ok(hash)
    }
}

/// One attempt at publishing a root with its nodes. It owns the nodes behind
/// an `Arc`, so a re-run after a conflict writes the same bytes again, and
/// then runs the caller's root write.
struct PublishNodes {
    writes: Arc<RapidHashMap<Hash, Vec<u8>>>,
    root: RetryAttempt,
}

impl RetryAttemptFn for PublishNodes {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            for (hash, bytes) in self.writes.iter() {
                txn.set(&node_key(hash), bytes).await?;
            }
            self.root.run(txn).await
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "prolly_nodes_tests.rs"]
mod tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "prolly_nodes_publish_tests.rs"]
mod publish_tests;
