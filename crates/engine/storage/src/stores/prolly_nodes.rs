//! Prolly tree nodes in regolith, one ContentAddressed key per node
//! (`proofs/tla/ProllyNodeCache.tla`).
//!
//! A node is stored under `t` and the CID of its encoding (raw codec,
//! SHA-256), so regolith classifies every node key as content addressed: a
//! put is blind and never conflicts, and a read is never validated. A node
//! never changes once named, so any copy of one, here or in a
//! [`prolly::Cached`] in front of this store, is never stale.
//!
//! Pending nodes and the root list commit together. Reclamation reads that
//! same list with a validated read before deleting unmarked node keys.

use std::sync::Arc;

use cid::{multihash::Multihash, Cid};
use prolly::{Hash, Node};
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};

use crate::corekv::{
    engine_retry_options, transact, IterOptions, MaybeSendSync, RetryAttemptError, RetryAttemptFn,
    RetryAttemptFuture, RetryAttemptValue, RetryOptions, RetryTxn, Store,
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

// An Ordinary key so root-list changes invalidate a reclaim decision.
const ROOTS_KEY: &[u8] = b"x/prolly-roots";

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

type LiveRootsFuture<'txn> = MaybeBoxFuture<'txn, crate::corekv::Result<Vec<Hash>>>;

trait LiveRoots: MaybeSendSync {
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
    /// Replaces the live roots and writes the nodes they need in one commit.
    /// A failed publication leaves pending nodes available for another attempt.
    pub async fn publish(&mut self, roots: &[Hash]) -> Result<(), RetryAttemptError> {
        let attempt = Box::new(PublishNodes {
            writes: Arc::clone(&self.pending),
            roots: roots.to_vec(),
        });
        transact::<_, ()>(&*self.store, self.retry, None, attempt).await?;
        self.pending = Arc::new(RapidHashMap::new());
        self.pending_bytes = 0;
        Ok(())
    }

    /// Deletes unmarked nodes from one page of the node keyspace.
    pub async fn reclaim(
        store: &S,
        start: Option<Vec<u8>>,
        limit: usize,
    ) -> prolly::Result<ReclaimPage> {
        Self::reclaim_with_roots(store, Arc::new(StoredRoots), start, limit).await
    }

    async fn reclaim_with_roots<R>(
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
    let cid = Cid::try_from(key.strip_prefix(&[PREFIX])?).ok()?;
    if cid.version() != cid::Version::V1 || cid.codec() != RAW || cid.hash().code() != SHA2_256 {
        return None;
    }
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
    for _ in 0..limit {
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

async fn read_roots(txn: &dyn RetryTxn) -> crate::corekv::Result<Vec<Hash>> {
    let bytes = txn.get(ROOTS_KEY).await?.unwrap_or_default();
    bytes
        .chunks(32)
        .map(|bytes| Hash::from_slice(bytes).map_err(node_error))
        .collect()
}

struct StoredRoots;

impl LiveRoots for StoredRoots {
    fn read<'txn>(&'txn self, txn: &'txn mut dyn RetryTxn) -> LiveRootsFuture<'txn> {
        Box::pin(async move { read_roots(txn).await })
    }
}

fn node_error(error: prolly::Error) -> crate::corekv::Error {
    crate::corekv::Error::Other(error.to_string())
}

fn add_children(bytes: &[u8], pending: &mut Vec<Hash>) -> crate::corekv::Result<()> {
    let node = Node::decode(bytes).map_err(node_error)?;
    if node.level > 0 {
        for item in node.items {
            pending.push(Hash::from_slice(&item.value).map_err(node_error)?);
        }
    }
    Ok(())
}

async fn read_node(txn: &dyn RetryTxn, hash: Hash) -> crate::corekv::Result<bytes::Bytes> {
    txn.get(&node_key(&hash))
        .await?
        .ok_or_else(|| node_error(prolly::Error::MissingNode(hash)))
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
        let bytes = read_node(txn, hash).await?;
        add_children(&bytes, &mut pending)?;
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

struct PublishNodes {
    writes: Arc<RapidHashMap<Hash, Vec<u8>>>,
    roots: Vec<Hash>,
}

impl RetryAttemptFn for PublishNodes {
    fn run<'txn>(&'txn mut self, txn: &'txn mut dyn RetryTxn) -> RetryAttemptFuture<'txn> {
        Box::pin(async move {
            let previous = read_roots(txn).await?;
            let mut fringe = self.roots.clone();
            let mut kept = if fringe.is_empty() {
                RapidHashSet::new()
            } else {
                mark_roots(txn, previous).await?
            };
            while let Some(hash) = fringe.pop() {
                if !kept.insert(hash) {
                    continue;
                }
                let stored;
                let bytes = if let Some(bytes) = self.writes.get(&hash) {
                    bytes.as_slice()
                } else {
                    // An unmarked node can disappear before this commit.
                    stored = read_node(txn, hash).await?;
                    &stored
                };
                add_children(bytes, &mut fringe)?;
                txn.set(&node_key(&hash), bytes).await?;
            }
            let encoded: Vec<_> = self.roots.iter().flat_map(|root| root.0).collect();
            txn.set(ROOTS_KEY, &encoded).await?;
            Ok(Box::new(()) as RetryAttemptValue)
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "prolly_nodes_tests.rs"]
mod tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "prolly_nodes_publish_tests.rs"]
mod publish_tests;
