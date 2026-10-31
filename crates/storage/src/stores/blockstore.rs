use crate::corekv::{
    IterOptions, Iterator, Key, Reader, Result, RetryAttemptFn, RetryAttemptFuture, RetryOptions,
    RetryTxn, Store, Txn, Writer,
};
use crate::keys::blockstore::{BlockstoreKey, ToMergeIndexKey};
use crate::namespace::{Namespace, NamespacedStore};
use async_trait::async_trait;
/// Blockstore - IPLD blocks and merkle tree nodes
///
/// The Blockstore handles storage of IPLD blocks with merge tracking for CRDT operations.
/// It tracks which blocks have been merged into the permanent store vs. pending merge.
use bytes::Bytes;
use cid::Cid;
use std::sync::Arc;

/// Blockstore provides storage for IPLD blocks with merge tracking
pub struct Blockstore<S: Store> {
    store: NamespacedStore<S>,
    /// Whether this is a P2P blockstore (affects merge tracking behavior)
    is_p2p: bool,
}

impl<S: Store> Clone for Blockstore<S> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            is_p2p: self.is_p2p,
        }
    }
}

impl<S: Store> Blockstore<S> {
    /// Create a new Blockstore with specified namespace
    pub fn new_with_namespace(store: Arc<S>, is_p2p: bool, namespace: Namespace) -> Self {
        Self {
            store: NamespacedStore::new(store, namespace),
            is_p2p,
        }
    }

    /// Create a new Blockstore (uses Blockstore namespace by default)
    pub fn new(store: Arc<S>, is_p2p: bool) -> Self {
        Self::new_with_namespace(store, is_p2p, Namespace::Blockstore)
    }

    /// Run `writes` in one transaction whose conflicts the engine re-runs
    /// under `retry`. Each re-run reads the blocks again, so a block another
    /// writer stored first is left as it is. Returns, per write, whether it
    /// found or changed a block: false for a put of a block already stored and
    /// for a merge mark of a block not stored.
    pub async fn write_blocks(
        &self,
        retry: RetryOptions,
        writes: Vec<BlockWrite>,
    ) -> Result<Vec<bool>> {
        let attempt = Box::new(BlockWritesAttempt {
            writes,
            is_p2p: self.is_p2p,
        });
        self.store
            .transact_retry(retry, attempt)
            .await
            .map_err(crate::corekv::RetryAttemptError::into_storage)?
            .downcast::<Vec<bool>>()
            .map(|written| *written)
            .map_err(|_| crate::corekv::Error::Other("invalid block write result".into()))
    }
}

/// One write [`Blockstore::write_blocks`] runs.
pub enum BlockWrite {
    /// Store a block if it is not stored yet; in P2P mode it starts unmerged.
    Put { cid: Cid, data: Bytes },
    /// Mark a block merged.
    MarkMerged(Cid),
    /// Delete a block and its merge mark.
    Delete(Cid),
}

/// The keys a put writes: the block, and in P2P mode its unmerged mark.
fn put_keys(cid: Cid, is_p2p: bool) -> (Vec<u8>, Option<Vec<u8>>) {
    (
        BlockstoreKey::new(cid).bytes(),
        is_p2p.then(|| ToMergeIndexKey::new(cid).bytes()),
    )
}

/// A to-merge mark's value: the time the block was fetched, as big-endian
/// unix seconds (Go Shinzo's `newToMergeValue`). The orphan sweep reads it;
/// whether a block is merged depends only on the mark being there.
pub fn marker_value(unix_secs: u64) -> [u8; 8] {
    unix_secs.to_be_bytes()
}

/// The fetch time a mark holds. A value that is not eight bytes (the single
/// `0xff` byte older builds wrote) holds none, and the sweep leaves it.
pub fn marker_time(value: &[u8]) -> Option<u64> {
    <[u8; 8]>::try_from(value).ok().map(u64::from_be_bytes)
}

/// A mark for a block fetched now.
fn fresh_marker() -> [u8; 8] {
    use web_time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    marker_value(now.as_secs())
}

/// Store `data` under `cid`, or, when it is stored already, renew its mark
/// if it is still waiting to merge, so a block that keeps arriving is not
/// swept as abandoned. A merged block stays merged: the mark is read in this
/// transaction, so a merge clearing it first makes this attempt run again.
/// Evaluates to whether the block was newly stored. A macro, as the retry
/// transaction and [`BlockstoreTxn`] share no reader and writer trait.
macro_rules! put_or_refresh {
    ($txn:expr, $cid:expr, $data:expr, $is_p2p:expr) => {{
        let (block_key, merge_key) = put_keys($cid, $is_p2p);
        if $txn.has(&block_key).await? {
            if let Some(merge_key) = merge_key {
                if $txn.has(&merge_key).await? {
                    $txn.set(&merge_key, &fresh_marker()).await?;
                }
            }
            false
        } else {
            $txn.set(&block_key, $data).await?;
            if let Some(merge_key) = merge_key {
                $txn.set(&merge_key, &fresh_marker()).await?;
            }
            true
        }
    }};
}

struct BlockWritesAttempt {
    writes: Vec<BlockWrite>,
    is_p2p: bool,
}

impl RetryAttemptFn for BlockWritesAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let mut found = Vec::with_capacity(self.writes.len());
            for write in &self.writes {
                found.push(match write {
                    BlockWrite::Put { cid, data } => {
                        put_or_refresh!(txn, *cid, data, self.is_p2p)
                    }
                    BlockWrite::MarkMerged(cid) => {
                        let stored = txn.has(&BlockstoreKey::new(*cid).bytes()).await?;
                        txn.delete(&ToMergeIndexKey::new(*cid).bytes()).await?;
                        stored
                    }
                    BlockWrite::Delete(cid) => {
                        txn.delete(&BlockstoreKey::new(*cid).bytes()).await?;
                        txn.delete(&ToMergeIndexKey::new(*cid).bytes()).await?;
                        true
                    }
                });
            }
            Ok(Box::new(found) as crate::corekv::RetryAttemptValue)
        })
    }
}

impl<S: Store> crate::corekv::private::Sealed for Blockstore<S> {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store> Store for Blockstore<S> {
    fn crdts(&self) -> &crdt::Crdts {
        self.store.crdts()
    }

    async fn allocate(&self, key: &[u8], count: u64) -> Result<std::ops::Range<u64>> {
        self.store.allocate(key, count).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn transaction_stats_handle(&self) -> Option<crate::backends::TransactionStatsHandle> {
        self.store.transaction_stats_handle()
    }

    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>> {
        let txn = self.store.new_txn(readonly).await?;
        Ok(Box::new(BlockstoreTxn {
            txn,
            is_p2p: self.is_p2p,
        }))
    }

    async fn transact_retry(
        &self,
        options: crate::corekv::RetryOptions,
        attempt: crate::corekv::RetryAttempt,
    ) -> std::result::Result<crate::corekv::RetryAttemptValue, crate::corekv::RetryAttemptError>
    {
        self.store.transact_retry(options, attempt).await
    }

    async fn close(&self) -> Result<()> {
        self.store.close().await
    }
}

/// Blockstore transaction with merge tracking
pub struct BlockstoreTxn {
    txn: Box<dyn Txn>,
    is_p2p: bool,
}

impl BlockstoreTxn {
    /// Put a block with automatic merge tracking: a new block starts unmerged
    /// in P2P mode, a stored one still waiting to merge has its mark renewed,
    /// and a merged one stays merged.
    pub async fn put_block(&mut self, cid: &Cid, data: &[u8]) -> Result<()> {
        let is_p2p = self.is_p2p;
        put_or_refresh!(self, *cid, data, is_p2p);
        Ok(())
    }

    /// Get a block by CID
    pub async fn get_block(&self, cid: &Cid) -> Result<Option<Bytes>> {
        let block_key = BlockstoreKey::new(*cid);
        self.get(&block_key.bytes()).await
    }

    /// Check if a block exists
    pub async fn has_block(&self, cid: &Cid) -> Result<bool> {
        let block_key = BlockstoreKey::new(*cid);
        self.has(&block_key.bytes()).await
    }

    /// Check if a block has been merged
    pub async fn is_merged(&self, cid: &Cid) -> Result<bool> {
        let merge_key = ToMergeIndexKey::new(*cid);
        let has_merge_marker = self.has(&merge_key.bytes()).await?;
        // If the merge marker doesn't exist, the block is merged
        Ok(!has_merge_marker)
    }

    /// Mark a block as merged (removes from merge tracking)
    pub async fn mark_as_merged(&mut self, cid: &Cid) -> Result<()> {
        let merge_key = ToMergeIndexKey::new(*cid);
        self.delete(&merge_key.bytes()).await
    }

    /// Get all unmerged block CIDs
    ///
    /// Returns a list of CIDs for blocks that have not yet been merged.
    /// If any keys fail to parse, returns an error to prevent silent data loss.
    /// The error includes the count of successfully parsed CIDs for debugging.
    pub async fn get_unmerged_cids(&self) -> Result<Vec<Cid>> {
        let mut cids = Vec::new();
        let mut parse_errors = 0;

        let opts = IterOptions::new().with_prefix(ToMergeIndexKey::merge_prefix());

        let mut iter = self.iterator(opts).await?;
        while let Some(pair) = iter.next().await? {
            // Parse the key to extract CID
            match ToMergeIndexKey::from_bytes(&pair.key) {
                Ok(merge_key) => {
                    cids.push(merge_key.cid);
                }
                Err(e) => {
                    parse_errors += 1;
                    tracing::error!(
                        key_bytes = ?pair.key,
                        error = %e,
                        "Failed to parse merge index key - possible data corruption"
                    );
                }
            }
        }

        // Return error if any keys failed to parse - this indicates data corruption
        // and the caller should be aware that the result is incomplete
        if parse_errors > 0 {
            return Err(crate::corekv::Error::Other(format!(
                "Data corruption detected: {} merge index keys could not be parsed (recovered {} CIDs). \
                This may indicate storage corruption or a version mismatch.",
                parse_errors,
                cids.len()
            )));
        }

        Ok(cids)
    }

    /// Delete a block and its merge tracking
    pub async fn delete_block(&mut self, cid: &Cid) -> Result<()> {
        let block_key = BlockstoreKey::new(*cid);
        self.delete(&block_key.bytes()).await?;

        // Also delete merge tracking if it exists
        let merge_key = ToMergeIndexKey::new(*cid);
        if self.has(&merge_key.bytes()).await? {
            self.delete(&merge_key.bytes()).await?;
        }

        Ok(())
    }
}

impl crate::corekv::private::Sealed for BlockstoreTxn {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Reader for BlockstoreTxn {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.txn.get(key).await
    }

    async fn get_parts(&self, key: &[u8], parts: &[u32]) -> Result<Option<Bytes>> {
        self.txn.get_parts(key, parts).await
    }

    async fn has(&self, key: &[u8]) -> Result<bool> {
        self.txn.has(key).await
    }

    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        self.txn.get_size(key).await
    }

    async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>> {
        self.txn.iterator(opts).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Writer for BlockstoreTxn {
    async fn merge_shared(&self, key: &[u8], operand: &[u8]) -> Result<()> {
        self.txn.merge_shared(key, operand).await
    }

    async fn append_shared(
        &self,
        log: &crate::corekv::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> Result<()> {
        self.txn.append_shared(log, entry, once_key).await
    }

    async fn truncate_log_shared(
        &self,
        log: &crate::corekv::AppendLog,
        positions: std::ops::RangeInclusive<u64>,
    ) -> Result<()> {
        self.txn.truncate_log_shared(log, positions).await
    }

    async fn appended(&self, log: &crate::corekv::AppendLog) -> Result<Vec<Vec<u8>>> {
        self.txn.appended(log).await
    }

    async fn set_shared(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.txn.set_shared(key, value).await
    }

    async fn delete_shared(&self, key: &[u8]) -> Result<()> {
        self.txn.delete_shared(key).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Txn for BlockstoreTxn {
    fn set_savepoint(&mut self) -> Result<()> {
        self.txn.set_savepoint()
    }

    fn rollback_to_savepoint(&mut self) -> Result<()> {
        self.txn.rollback_to_savepoint()
    }

    fn release_savepoint(&mut self) -> Result<()> {
        self.txn.release_savepoint()
    }

    async fn commit(self: Box<Self>) -> Result<()> {
        self.txn.commit().await
    }

    fn discard(self: Box<Self>) {
        self.txn.discard()
    }

    fn on_commit(&mut self, callback: crate::corekv::OnCommit) -> Result<()> {
        self.txn.on_commit(callback)
    }

    fn on_abort(&mut self, callback: crate::corekv::OnAbort) {
        self.txn.on_abort(callback)
    }

    fn before_commit(&mut self, callback: crate::corekv::BeforeCommit) {
        self.txn.before_commit(callback)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn is_readonly(&self) -> bool {
        self.txn.is_readonly()
    }

    fn callback_count(&self) -> usize {
        self.txn.callback_count()
    }
}

// Tests extracted to crates/storage/tests/blockstore_tests.rs
