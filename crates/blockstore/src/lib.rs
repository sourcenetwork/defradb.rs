//! IPFS-compatible blockstore with merge tracking for DefraDB
//!
//! This crate provides a public API layer for IPLD block storage with
//! CRDT merge tracking support for P2P synchronization.
//!
//! # Architecture
//!
//! ```text
//! Application Layer
//!     ↓
//! Blockstore Trait (this crate - public API)
//!     ↓
//! storage::stores::Blockstore (internal implementation)
//!     ↓
//! CoreKV Backend (Memory/RocksDB)
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use blockstore::{Blockstore, DefraBlockstore};
//! use storage::RegolithStore;
//! use std::sync::Arc;
//!
//! // Create a blockstore
//! let store = Arc::new(RegolithStore::in_memory().unwrap());
//! let blockstore = DefraBlockstore::new(store, false);
//!
//! // Store a block
//! let cid = /* compute CID from data */;
//! blockstore.put(&cid, b"block data").await?;
//!
//! // Retrieve a block
//! let data = blockstore.get(&cid).await?;
//!
//! // Enable hash verification on read
//! blockstore.hash_on_read(true);
//! let verified_data = blockstore.get(&cid).await?; // Will verify hash
//! ```

mod error;
mod traits;
mod verify;

pub use error::{Error, Result};
pub use traits::Blockstore;
pub use verify::verify_block_cid;

use async_trait::async_trait;
use bytes::Bytes;
use cid::Cid;
use multihash_codetable::Code;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use storage::corekv::{IterOptions, Key, RetryOptions, Store};
use storage::keys::blockstore::{BlockstoreKey, ToMergeIndexKey};
use storage::stores::blockstore::BlockstoreTxn;
use storage::stores::BlockWrite;
use storage::stores::Blockstore as InternalBlockstore;
use tracing::instrument;

/// DefraDB blockstore implementation
///
/// Wraps the internal storage::stores::Blockstore with a clean public API.
/// Supports both P2P mode (with merge tracking) and local mode (no tracking).
///
/// It keeps no cache of its own: every read goes to the engine, whose block
/// cache serves a repeated read, so every handle over one store sees the same
/// blocks and merge marks. A read is zero-copy: the `Bytes` it returns hold
/// the engine's own bytes, which stay unchanged for as long as they are held,
/// whatever the store does after.
pub struct DefraBlockstore<S: Store> {
    store: InternalBlockstore<S>,
    /// Whether to verify hash on read
    rehash: AtomicBool,
    /// The engine retry policy every block write runs under.
    retry: RetryOptions,
    /// The worker pool its reads and writes run on, for a caller off it (the
    /// P2P layer on the network edge).
    pool: Option<workers::Pool>,
}

impl<S: Store> fmt::Debug for DefraBlockstore<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DefraBlockstore")
            .field("rehash", &self.rehash.load(Ordering::Relaxed))
            .finish()
    }
}

impl<S: Store + 'static> DefraBlockstore<S> {
    /// Create a new blockstore
    ///
    /// # Arguments
    ///
    /// * `store` - The underlying key-value store
    /// * `is_p2p` - If true, enables merge tracking for P2P synchronization.
    ///   Blocks put in P2P mode are initially marked as "unmerged"
    ///   until explicitly merged via `mark_as_merged`.
    pub fn new(store: Arc<S>, is_p2p: bool) -> Self {
        Self {
            store: InternalBlockstore::new(store, is_p2p),
            rehash: AtomicBool::new(false),
            retry: storage::corekv::engine_retry_options(storage::corekv::DEFAULT_MAX_TXN_RETRIES),
            pool: None,
        }
    }

    /// Runs its reads and writes on `pool`: a caller off the pool hands each
    /// to a worker, a worker makes it in place.
    pub fn placed_on(mut self, pool: Option<&workers::Pool>) -> Self {
        self.pool = pool.cloned();
        self
    }

    /// Runs `call` with the store on the pool the blockstore is placed on: in
    /// place on a worker, or without a pool.
    #[cfg(not(target_arch = "wasm32"))]
    async fn on_pool<T, F, Fut>(&self, call: F) -> Result<T>
    where
        F: FnOnce(InternalBlockstore<S>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T>> + 'static,
        T: Send + 'static,
    {
        let store = self.store.clone();
        workers::run_on(
            self.pool.as_ref(),
            workers::Target::LeastLoaded,
            move || call(store),
        )
        .await
        .map_err(|error| Error::Internal(format!("the block store call did not run: {error}")))?
    }

    /// A browser's one worker is the page's thread: the call runs in place.
    #[cfg(target_arch = "wasm32")]
    async fn on_pool<T, F, Fut>(&self, call: F) -> Result<T>
    where
        F: FnOnce(InternalBlockstore<S>) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        call(self.store.clone()).await
    }

    /// Writes `writes` in one engine transaction, on the pool.
    async fn write(&self, writes: Vec<BlockWrite>) -> Result<()> {
        let retry = self.retry;
        self.on_pool(move |store| async move {
            store.write_blocks(retry, writes).await?;
            Ok(())
        })
        .await
    }

    /// Re-run a conflicting block write up to `max_txn_retries` times, the
    /// node's `datastore.max_txn_retries`.
    pub fn with_max_txn_retries(mut self, max_txn_retries: u32) -> Self {
        self.retry = storage::corekv::engine_retry_options(max_txn_retries);
        self
    }

    /// Check if hash verification on read is currently enabled
    pub fn rehash_enabled(&self) -> bool {
        self.rehash.load(Ordering::Relaxed)
    }

    /// Create a new transaction on the underlying store (for testing)
    pub async fn new_store_txn(&self, read_only: bool) -> Result<Box<dyn storage::corekv::Txn>> {
        Ok(self.store.new_txn(read_only).await?)
    }

    /// Verify that data matches the CID hash
    ///
    /// # Supported Algorithms
    ///
    /// Currently supports SHA2-256 (code 0x12), which is the most common hash
    /// algorithm used in IPFS/IPLD and DefraDB.
    ///
    /// # Go Compatibility Note
    ///
    /// The Go implementation uses `cid.Prefix().Sum(data)` which delegates to the
    /// go-multihash library and supports all registered hash algorithms. This Rust
    /// implementation explicitly handles SHA2-256 only. Unsupported algorithms are
    /// logged and skipped (verification passes) rather than erroring, matching the
    /// principle of being permissive on read. If DefraDB ever uses non-SHA256 hashes,
    /// this function should be extended to support them.
    fn verify_hash(&self, cid: &Cid, data: &[u8]) -> Result<()> {
        use sha2::{Digest, Sha256};

        let mh = cid.hash();
        let code = mh.code();

        // Compute hash based on the multihash code
        if code == u64::from(Code::Sha2_256) {
            let mut hasher = Sha256::new();
            hasher.update(data);
            let computed_digest = hasher.finalize();

            if mh.digest() != computed_digest.as_slice() {
                return Err(Error::HashMismatch {
                    cid: cid.to_string(),
                });
            }
            Ok(())
        } else {
            tracing::warn!(
                hash_code = code,
                cid = %cid,
                "Hash verification skipped: unsupported hash algorithm"
            );
            Ok(())
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static> Blockstore for DefraBlockstore<S> {
    #[instrument(level = "trace", skip(self))]
    async fn get(&self, cid: &Cid) -> Result<Option<Bytes>> {
        let at = *cid;
        let result = self
            .on_pool(move |store| async move {
                let txn = store.new_txn(true).await?;
                Ok(block_txn(&*txn)?.get_block(&at).await?)
            })
            .await?;

        if self.rehash.load(Ordering::Acquire) {
            if let Some(ref data) = result {
                self.verify_hash(cid, data)?;
            }
        }

        Ok(result)
    }

    #[instrument(level = "trace", skip(self))]
    async fn put(&self, cid: &Cid, data: &[u8]) -> Result<()> {
        self.write(vec![BlockWrite::Put {
            cid: *cid,
            data: Bytes::copy_from_slice(data),
        }])
        .await
    }

    async fn put_many(&self, blocks: &[(&Cid, &[u8])]) -> Result<()> {
        if blocks.is_empty() {
            return Ok(());
        }
        let writes = blocks
            .iter()
            .map(|(cid, data)| BlockWrite::Put {
                cid: **cid,
                data: Bytes::copy_from_slice(data),
            })
            .collect();
        self.write(writes).await
    }

    async fn has(&self, cid: &Cid) -> Result<bool> {
        let at = *cid;
        self.on_pool(move |store| async move {
            let txn = store.new_txn(true).await?;
            Ok(block_txn(&*txn)?.has_block(&at).await?)
        })
        .await
    }

    async fn has_many(&self, cids: &[Cid]) -> Result<Vec<bool>> {
        if cids.is_empty() {
            return Ok(Vec::new());
        }
        let cids = cids.to_vec();
        self.on_pool(move |store| async move {
            let txn = store.new_txn(true).await?;
            let bs_txn = block_txn(&*txn)?;
            let mut present = Vec::with_capacity(cids.len());
            for cid in &cids {
                present.push(bs_txn.has_block(cid).await?);
            }
            Ok(present)
        })
        .await
    }

    async fn delete(&self, cid: &Cid) -> Result<()> {
        self.write(vec![BlockWrite::Delete(*cid)]).await
    }

    async fn get_size(&self, cid: &Cid) -> Result<Option<usize>> {
        let block_key = BlockstoreKey::new(*cid);
        self.on_pool(move |store| async move {
            let txn = store.new_txn(true).await?;
            Ok(txn.get_size(&block_key.bytes()).await?)
        })
        .await
    }

    async fn all_cids(&self) -> Result<Vec<Cid>> {
        self.on_pool(|store| async move { all_cids(&store).await })
            .await
    }

    fn hash_on_read(&self, enabled: bool) {
        self.rehash.store(enabled, Ordering::Release);
    }

    async fn is_merged(&self, cid: &Cid) -> Result<bool> {
        let at = *cid;
        self.on_pool(move |store| async move {
            let txn = store.new_txn(true).await?;
            let bs_txn = block_txn(&*txn)?;
            if !bs_txn.has_block(&at).await? {
                return Ok(false);
            }
            Ok(bs_txn.is_merged(&at).await?)
        })
        .await
    }

    async fn mark_as_merged(&self, cid: &Cid) -> Result<()> {
        self.write(vec![BlockWrite::MarkMerged(*cid)]).await
    }

    async fn mark_batch_as_merged(&self, cids: &[Cid]) -> Result<()> {
        if cids.is_empty() {
            return Ok(());
        }
        self.write(cids.iter().copied().map(BlockWrite::MarkMerged).collect())
            .await
    }

    async fn get_unmerged(&self) -> Result<Vec<Cid>> {
        self.on_pool(|store| async move {
            let txn = store.new_txn(true).await?;
            Ok(block_txn(&*txn)?.get_unmerged_cids().await?)
        })
        .await
    }
}

/// The blockstore view of a transaction `InternalBlockstore::new_txn` opened.
fn block_txn(txn: &dyn storage::corekv::Txn) -> Result<&BlockstoreTxn> {
    txn.as_any()
        .downcast_ref::<BlockstoreTxn>()
        .ok_or_else(|| Error::Internal("Failed to downcast transaction".to_string()))
}

/// Every block's CID, skipping the merge markers.
async fn all_cids<S: Store>(store: &InternalBlockstore<S>) -> Result<Vec<Cid>> {
    let txn = store.new_txn(true).await?;

    // Iterate all keys that don't start with merge prefix
    // Block keys are raw CID bytes, merge keys start with 'm'
    let opts = IterOptions::new().with_keys_only(true);
    let mut iter = txn.iterator(opts).await?;

    let mut cids = Vec::new();
    while let Some(pair) = iter.next().await? {
        // Skip merge marker keys (start with 'm')
        if ToMergeIndexKey::is_merge_key(&pair.key) {
            continue;
        }

        // Parse the key as a CID
        match BlockstoreKey::from_bytes(&pair.key) {
            Ok(key) => cids.push(key.cid),
            Err(e) => {
                tracing::warn!(
                    key_bytes = ?pair.key,
                    error = %e,
                    "Skipping key that could not be parsed as CID"
                );
            }
        }
    }

    Ok(cids)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "placed_tests.rs"]
mod placed_tests;
