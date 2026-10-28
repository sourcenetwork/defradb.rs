//! A test-only [`Store`] decorator that counts real storage reads.
//!
//! Unlike counting documents handed to the query plan, this counts what a
//! query pulls through the [`Store`] API: keys yielded by an iterator, and
//! single-key reads. A fetcher that materializes an entire collection before
//! yielding a `LIMIT`-bounded slice cannot hide behind plan-level document
//! counts when instrumented with this decorator.
//!
//! The measurement point is the `Store` trait boundary. Whatever a backend
//! does to refill its own window happens inside its `Iterator` implementation,
//! below this decorator, and is not counted here.
//!
//! It also records the keys an explicit transaction reads by point, scans by
//! prefix and writes, so a test can say which keys a write path touched.
//! Engine-owned retry attempts (`transact_retry`) pass straight through and
//! are neither counted nor recorded.

use async_trait::async_trait;
use regolith::sync::SegQueue;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use storage::corekv::private::Sealed;
use storage::corekv::IterOptions;
use storage::corekv::Iterator;
use storage::corekv::KvPair;
use storage::corekv::Reader;
use storage::corekv::Result;
use storage::corekv::Store;
use storage::corekv::Txn;
use storage::corekv::Writer;

#[derive(Default)]
struct Counts {
    keys_read: AtomicUsize,
    point_gets: AtomicUsize,
    point_keys: SegQueue<Vec<u8>>,
    scan_prefixes: SegQueue<Vec<u8>>,
    written_keys: SegQueue<Vec<u8>>,
}

impl Counts {
    fn point(&self, key: &[u8]) {
        self.point_gets.fetch_add(1, Ordering::SeqCst);
        self.point_keys.push(key.to_vec());
    }
}

fn drain(queue: &SegQueue<Vec<u8>>) -> Vec<Vec<u8>> {
    std::iter::from_fn(|| queue.pop()).collect()
}

/// Wraps a store and counts what a query actually pulls out of it.
pub(crate) struct CountingStore<S: Store> {
    inner: S,
    counts: Arc<Counts>,
}

impl<S: Store> CountingStore<S> {
    pub(crate) fn new(inner: S) -> Self {
        Self {
            inner,
            counts: Arc::new(Counts::default()),
        }
    }

    /// Keys yielded by an iterator.
    pub(crate) fn keys_read(&self) -> usize {
        self.counts.keys_read.load(Ordering::SeqCst)
    }

    /// Single-key reads — `get`, `has` and `get_size` — which document
    /// assembly performs per document.
    pub(crate) fn point_gets(&self) -> usize {
        self.counts.point_gets.load(Ordering::SeqCst)
    }

    /// The keys read by point since the last call, oldest first.
    pub(crate) fn take_point_keys(&self) -> Vec<Vec<u8>> {
        drain(&self.counts.point_keys)
    }

    /// The prefix (empty without one) of each scan opened since the last call.
    pub(crate) fn take_scan_prefixes(&self) -> Vec<Vec<u8>> {
        drain(&self.counts.scan_prefixes)
    }

    /// The keys put, merged into or deleted since the last call.
    pub(crate) fn take_written_keys(&self) -> Vec<Vec<u8>> {
        drain(&self.counts.written_keys)
    }
}

impl<S: Store> Sealed for CountingStore<S> {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store> Store for CountingStore<S> {
    fn crdts(&self) -> &storage::Crdts {
        self.inner.crdts()
    }

    async fn allocate(&self, key: &[u8], count: u64) -> storage::Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn transaction_stats_handle(&self) -> Option<storage::backends::TransactionStatsHandle> {
        self.inner.transaction_stats_handle()
    }

    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>> {
        let inner = self.inner.new_txn(readonly).await?;
        Ok(Box::new(CountingTxn {
            inner,
            counts: Arc::clone(&self.counts),
        }))
    }

    async fn transact_retry(
        &self,
        options: storage::corekv::RetryOptions,
        attempt: storage::corekv::RetryAttempt,
    ) -> std::result::Result<storage::corekv::RetryAttemptValue, storage::corekv::RetryAttemptError>
    {
        self.inner.transact_retry(options, attempt).await
    }

    async fn close(&self) -> Result<()> {
        self.inner.close().await
    }
}

/// A [`Txn`] wrapper that counts point `get`s and hands out counting iterators.
struct CountingTxn {
    inner: Box<dyn Txn>,
    counts: Arc<Counts>,
}

impl Sealed for CountingTxn {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Reader for CountingTxn {
    async fn get(&self, key: &[u8]) -> Result<Option<bytes::Bytes>> {
        self.counts.point(key);
        self.inner.get(key).await
    }

    async fn get_parts(&self, key: &[u8], parts: &[u32]) -> Result<Option<bytes::Bytes>> {
        self.counts.point(key);
        self.inner.get_parts(key, parts).await
    }

    async fn has(&self, key: &[u8]) -> Result<bool> {
        self.counts.point(key);
        self.inner.has(key).await
    }

    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        self.counts.point(key);
        self.inner.get_size(key).await
    }

    async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>> {
        self.counts
            .scan_prefixes
            .push(opts.prefix().map(<[u8]>::to_vec).unwrap_or_default());
        Ok(Box::new(CountingIterator {
            inner: self.inner.iterator(opts).await?,
            counts: Arc::clone(&self.counts),
        }))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Writer for CountingTxn {
    async fn merge_shared(&self, key: &[u8], operand: &[u8]) -> Result<()> {
        self.counts.written_keys.push(key.to_vec());
        self.inner.merge_shared(key, operand).await
    }

    async fn append_shared(
        &self,
        log: &storage::corekv::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> storage::Result<()> {
        self.inner.append_shared(log, entry, once_key).await
    }

    async fn set_shared(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.counts.written_keys.push(key.to_vec());
        self.inner.set_shared(key, value).await
    }

    async fn delete_shared(&self, key: &[u8]) -> Result<()> {
        self.counts.written_keys.push(key.to_vec());
        self.inner.delete_shared(key).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Txn for CountingTxn {
    async fn commit(self: Box<Self>) -> Result<()> {
        self.inner.commit().await
    }

    fn set_savepoint(&mut self) -> Result<()> {
        self.inner.set_savepoint()
    }

    fn rollback_to_savepoint(&mut self) -> Result<()> {
        self.inner.rollback_to_savepoint()
    }

    fn release_savepoint(&mut self) -> Result<()> {
        self.inner.release_savepoint()
    }

    fn discard(self: Box<Self>) {
        self.inner.discard()
    }

    fn on_commit(&mut self, callback: storage::corekv::OnCommit) -> Result<()> {
        self.inner.on_commit(callback)
    }

    fn on_abort(&mut self, callback: storage::corekv::OnAbort) {
        self.inner.on_abort(callback)
    }

    fn before_commit(&mut self, callback: storage::corekv::BeforeCommit) {
        self.inner.before_commit(callback)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn is_readonly(&self) -> bool {
        self.inner.is_readonly()
    }

    fn callback_count(&self) -> usize {
        self.inner.callback_count()
    }
}

/// An [`Iterator`] wrapper that counts each key yielded by `next`.
struct CountingIterator {
    inner: Box<dyn Iterator>,
    counts: Arc<Counts>,
}

impl Sealed for CountingIterator {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Iterator for CountingIterator {
    async fn next(&mut self) -> Result<Option<KvPair>> {
        let pulled = self.inner.next().await?;
        if pulled.is_some() {
            self.counts.keys_read.fetch_add(1, Ordering::SeqCst);
        }
        Ok(pulled)
    }

    async fn close(&mut self) -> Result<()> {
        self.inner.close().await
    }

    async fn seek(&mut self, key: &[u8]) -> Result<bool> {
        self.inner.seek(key).await
    }

    async fn reset(&mut self) -> Result<()> {
        self.inner.reset().await
    }

    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}
