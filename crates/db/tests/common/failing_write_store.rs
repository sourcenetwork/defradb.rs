//! A test-only [`Store`] decorator that refuses the writes a predicate names,
//! in plain transactions and in engine-owned retry attempts alike.
//!
//! The predicate sees the raw key, namespace byte first (`b's'` for the
//! systemstore, `b'd'` for the datastore), and the value (`None` for a
//! delete); an error refuses the write.

use std::sync::Arc;

use async_trait::async_trait;
use storage::corekv::{
    intercept_retry_writes, private::Sealed, IterOptions, Iterator, OnAbort, OnCommit, Reader,
    Result, RetryAttempt, RetryAttemptError, RetryAttemptValue, RetryOptions,
    RetryWriteInterceptor, Store, Txn, Writer,
};
use storage::RegolithStore;

pub struct FailingWriteStore {
    inner: RegolithStore,
    refuse: RetryWriteInterceptor,
}

impl FailingWriteStore {
    /// An in-memory store whose writes `refuse` turns down.
    pub fn in_memory(refuse: RetryWriteInterceptor) -> Self {
        Self {
            inner: RegolithStore::in_memory().unwrap(),
            refuse,
        }
    }
}

impl Sealed for FailingWriteStore {}

#[async_trait]
impl Store for FailingWriteStore {
    fn crdts(&self) -> &storage::Crdts {
        self.inner.crdts()
    }

    async fn allocate(&self, key: &[u8], count: u64) -> storage::Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>> {
        Ok(Box::new(FailingWriteTxn {
            inner: self.inner.new_txn(readonly).await?,
            refuse: Arc::clone(&self.refuse),
        }))
    }

    async fn transact_retry(
        &self,
        options: RetryOptions,
        attempt: RetryAttempt,
    ) -> std::result::Result<RetryAttemptValue, RetryAttemptError> {
        self.inner
            .transact_retry(
                options,
                intercept_retry_writes(attempt, Arc::clone(&self.refuse)),
            )
            .await
    }

    async fn close(&self) -> Result<()> {
        self.inner.close().await
    }
}

struct FailingWriteTxn {
    inner: Box<dyn Txn>,
    refuse: RetryWriteInterceptor,
}

impl Sealed for FailingWriteTxn {}

#[async_trait]
impl Reader for FailingWriteTxn {
    async fn get(&self, key: &[u8]) -> Result<Option<bytes::Bytes>> {
        self.inner.get(key).await
    }

    async fn get_parts(&self, key: &[u8], parts: &[u32]) -> Result<Option<bytes::Bytes>> {
        self.inner.get_parts(key, parts).await
    }

    async fn has(&self, key: &[u8]) -> Result<bool> {
        self.inner.has(key).await
    }

    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        self.inner.get_size(key).await
    }

    async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>> {
        self.inner.iterator(opts).await
    }
}

#[async_trait]
impl Writer for FailingWriteTxn {
    async fn merge_shared(&self, key: &[u8], operand: &[u8]) -> Result<()> {
        (self.refuse)(key, Some(operand))?;
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
        (self.refuse)(key, Some(value))?;
        self.inner.set_shared(key, value).await
    }

    async fn delete_shared(&self, key: &[u8]) -> Result<()> {
        (self.refuse)(key, None)?;
        self.inner.delete_shared(key).await
    }
}

#[async_trait]
impl Txn for FailingWriteTxn {
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

    fn on_commit(&mut self, callback: OnCommit) -> Result<()> {
        self.inner.on_commit(callback)
    }

    fn on_abort(&mut self, callback: OnAbort) {
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
