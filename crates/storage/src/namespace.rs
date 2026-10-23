use crate::corekv::{Error, IterOptions, Iterator, KvPair, Reader, Result, Store, Txn, Writer};
use async_trait::async_trait;
/// Namespace isolation for multi-store architectures
///
/// This module provides byte-prefix namespacing to logically separate
/// stores within a single backend. Each store gets a single-byte prefix:
/// - 'd' (0x64): Datastore
/// - 'b' (0x62): Blockstore
/// - 'h' (0x68): Headstore
/// - 's' (0x73): Systemstore
/// - 'p' (0x70): Peerstore
/// - 'e' (0x65): Encstore
/// - 'a' (0x61): Acpstore
///
/// The NamespacedStore wraps any Store implementation and automatically
/// prepends the prefix to all keys, ensuring complete isolation between stores.
use bytes::Bytes;
use std::sync::Arc;

/// Store namespace prefixes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Namespace {
    /// Datastore - document and collection data
    Datastore,
    /// Blockstore - IPLD blocks and merkle tree nodes
    Blockstore,
    /// Headstore - merkle tree heads and definitions
    Headstore,
    /// Systemstore - metadata and configuration
    Systemstore,
    /// Peerstore - peer and replication metadata
    Peerstore,
    /// Encstore - encrypted blocks
    Encstore,
    /// Acpstore - Access Control Policy tuples
    Acpstore,
}

impl Namespace {
    /// Get the byte prefix for this namespace
    pub fn prefix(&self) -> u8 {
        match self {
            Namespace::Datastore => b'd',
            Namespace::Blockstore => b'b',
            Namespace::Headstore => b'h',
            Namespace::Systemstore => b's',
            Namespace::Peerstore => b'p',
            Namespace::Encstore => b'e',
            Namespace::Acpstore => b'a',
        }
    }

    /// Get the namespace name
    pub fn name(&self) -> &'static str {
        match self {
            Namespace::Datastore => "datastore",
            Namespace::Blockstore => "blockstore",
            Namespace::Headstore => "headstore",
            Namespace::Systemstore => "systemstore",
            Namespace::Peerstore => "peerstore",
            Namespace::Encstore => "encstore",
            Namespace::Acpstore => "acpstore",
        }
    }

    /// Add prefix to a key
    pub(crate) fn prefix_key(&self, key: &[u8]) -> Vec<u8> {
        let mut prefixed = Vec::with_capacity(1 + key.len());
        prefixed.push(self.prefix());
        prefixed.extend_from_slice(key);
        prefixed
    }

    /// `opts` scoped to this namespace: every bound prefixed, and the
    /// namespace's own byte as the prefix when the caller gave none, so a scan
    /// never leaves the namespace even when it has a start or an end.
    pub(crate) fn scope(&self, opts: IterOptions) -> IterOptions {
        let scoped = opts.map_keys(|key| self.prefix_key(key));
        if scoped.prefix().is_some() {
            scoped
        } else {
            scoped.with_prefix(vec![self.prefix()])
        }
    }

    /// Remove prefix from a key (for internal use)
    pub(crate) fn unprefix_key<'a>(&self, key: &'a [u8]) -> Result<&'a [u8]> {
        if key.is_empty() {
            return Err(Error::EmptyKey);
        }
        if key[0] != self.prefix() {
            return Err(Error::Other(format!(
                "Key has wrong prefix: expected {}, got {}",
                self.prefix() as char,
                key[0] as char
            )));
        }
        Ok(&key[1..])
    }
}

/// A namespaced store that wraps a Store implementation and automatically
/// prefixes all keys with a namespace byte
pub struct NamespacedStore<S: Store> {
    store: Arc<S>,
    namespace: Namespace,
}

impl<S: Store> NamespacedStore<S> {
    /// Create a new namespaced store
    pub fn new(store: Arc<S>, namespace: Namespace) -> Self {
        Self { store, namespace }
    }

    /// Get the underlying store
    pub fn inner(&self) -> &Arc<S> {
        &self.store
    }

    /// Get the namespace
    pub fn namespace(&self) -> Namespace {
        self.namespace
    }
}

impl<S: Store> Clone for NamespacedStore<S> {
    fn clone(&self) -> Self {
        Self::new(Arc::clone(&self.store), self.namespace)
    }
}

impl<S: Store> crate::corekv::private::Sealed for NamespacedStore<S> {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store> Store for NamespacedStore<S> {
    fn crdts(&self) -> &crdt::Crdts {
        self.store.crdts()
    }

    async fn allocate(&self, key: &[u8], count: u64) -> Result<std::ops::Range<u64>> {
        self.store
            .allocate(&self.namespace.prefix_key(key), count)
            .await
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn transaction_stats_handle(&self) -> Option<crate::backends::TransactionStatsHandle> {
        self.store.transaction_stats_handle()
    }

    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>> {
        let txn = self.store.new_txn(readonly).await?;
        Ok(Box::new(NamespacedTxn {
            txn,
            namespace: self.namespace,
        }))
    }

    async fn transact_retry(
        &self,
        options: crate::corekv::RetryOptions,
        attempt: crate::corekv::RetryAttempt,
    ) -> std::result::Result<crate::corekv::RetryAttemptValue, crate::corekv::RetryAttemptError>
    {
        self.store
            .transact_retry(
                options,
                Box::new(NamespacedRetryAttempt {
                    attempt,
                    namespace: self.namespace,
                }),
            )
            .await
    }

    async fn close(&self) -> Result<()> {
        self.store.close().await
    }
}

struct NamespacedRetryTxn<'a> {
    txn: &'a mut dyn crate::corekv::RetryTxn,
    namespace: Namespace,
}

struct NamespacedRetryAttempt {
    attempt: crate::corekv::RetryAttempt,
    namespace: Namespace,
}

impl crate::corekv::RetryAttemptFn for NamespacedRetryAttempt {
    fn run<'a>(
        &'a mut self,
        txn: &'a mut dyn crate::corekv::RetryTxn,
    ) -> crate::corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            let mut namespaced = NamespacedRetryTxn {
                txn,
                namespace: self.namespace,
            };
            self.attempt.run(&mut namespaced).await
        })
    }
}

impl crate::corekv::private::Sealed for NamespacedRetryTxn<'_> {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl crate::corekv::RetryTxn for NamespacedRetryTxn<'_> {
    fn previous_conflict(&self) -> Option<crate::corekv::TransactionConflict> {
        self.txn.previous_conflict()
    }

    async fn get(&self, key: &[u8]) -> Result<Option<bytes::Bytes>> {
        self.txn.get(&self.namespace.prefix_key(key)).await
    }

    async fn has(&self, key: &[u8]) -> Result<bool> {
        self.txn.has(&self.namespace.prefix_key(key)).await
    }

    async fn set(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        self.txn.set(&self.namespace.prefix_key(key), value).await
    }

    async fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.txn.delete(&self.namespace.prefix_key(key)).await
    }

    fn begin_attempt_io(&mut self) -> Result<Box<dyn crate::corekv::RetryAttemptIO>> {
        Ok(Box::new(NamespacedRetryAttemptIO {
            inner: self.txn.begin_attempt_io()?,
            namespace: self.namespace,
        }))
    }

    fn finish_attempt_io(
        &mut self,
        io: Box<dyn crate::corekv::RetryAttemptIO>,
        callbacks: crate::corekv::TxnCallbacks,
    ) -> Result<()> {
        let namespaced = io
            .into_any()
            .downcast::<NamespacedRetryAttemptIO>()
            .map_err(|_| Error::Other("invalid namespaced retry IO".into()))?;
        if namespaced.namespace != self.namespace {
            return Err(Error::Other("retry IO namespace mismatch".into()));
        }
        self.txn.finish_attempt_io(namespaced.inner, callbacks)
    }
}

struct NamespacedRetryAttemptIO {
    inner: Box<dyn crate::corekv::RetryAttemptIO>,
    namespace: Namespace,
}

impl crate::corekv::private::Sealed for NamespacedRetryAttemptIO {}

impl NamespacedRetryAttemptIO {
    fn inner(&self) -> &dyn crate::corekv::RetryAttemptIO {
        self.inner.as_ref()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl crate::corekv::RetryAttemptIO for NamespacedRetryAttemptIO {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Reader for NamespacedRetryAttemptIO {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.inner().get(&self.namespace.prefix_key(key)).await
    }

    async fn get_parts(&self, key: &[u8], parts: &[u32]) -> Result<Option<Bytes>> {
        self.inner()
            .get_parts(&self.namespace.prefix_key(key), parts)
            .await
    }

    async fn has(&self, key: &[u8]) -> Result<bool> {
        self.inner().has(&self.namespace.prefix_key(key)).await
    }

    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        self.inner().get_size(&self.namespace.prefix_key(key)).await
    }

    async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>> {
        Ok(Box::new(NamespacedIterator {
            iter: self.inner().iterator(self.namespace.scope(opts)).await?,
            namespace: self.namespace,
        }))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Writer for NamespacedRetryAttemptIO {
    async fn merge_shared(&self, key: &[u8], operand: &[u8]) -> Result<()> {
        let key = self.namespace.prefix_key(key);
        self.inner().merge_shared(&key, operand).await
    }

    async fn append_shared(
        &self,
        log: &crate::corekv::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> Result<()> {
        let log = log.prefixed(&[self.namespace.prefix()]);
        let once_key = once_key.map(|key| self.namespace.prefix_key(key));
        self.inner()
            .append_shared(&log, entry, once_key.as_deref())
            .await
    }

    async fn set_shared(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let key = self.namespace.prefix_key(key);
        self.inner().set_shared(&key, value).await
    }

    async fn delete_shared(&self, key: &[u8]) -> Result<()> {
        let key = self.namespace.prefix_key(key);
        self.inner().delete_shared(&key).await
    }
}

/// A namespaced transaction that wraps a Txn implementation
pub struct NamespacedTxn {
    txn: Box<dyn Txn>,
    namespace: Namespace,
}

impl NamespacedTxn {
    /// Create a new namespaced transaction
    pub fn new(txn: Box<dyn Txn>, namespace: Namespace) -> Self {
        Self { txn, namespace }
    }
}

impl crate::corekv::private::Sealed for NamespacedTxn {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Reader for NamespacedTxn {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        let prefixed = self.namespace.prefix_key(key);
        self.txn.get(&prefixed).await
    }

    async fn get_parts(&self, key: &[u8], parts: &[u32]) -> Result<Option<Bytes>> {
        let prefixed = self.namespace.prefix_key(key);
        self.txn.get_parts(&prefixed, parts).await
    }

    async fn has(&self, key: &[u8]) -> Result<bool> {
        let prefixed = self.namespace.prefix_key(key);
        self.txn.has(&prefixed).await
    }

    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        let prefixed = self.namespace.prefix_key(key);
        self.txn.get_size(&prefixed).await
    }

    async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>> {
        let iter = self.txn.iterator(self.namespace.scope(opts)).await?;
        Ok(Box::new(NamespacedIterator {
            iter,
            namespace: self.namespace,
        }))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Writer for NamespacedTxn {
    async fn merge_shared(&self, key: &[u8], operand: &[u8]) -> Result<()> {
        self.txn
            .merge_shared(&self.namespace.prefix_key(key), operand)
            .await
    }

    async fn append_shared(
        &self,
        log: &crate::corekv::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> Result<()> {
        let log = log.prefixed(&[self.namespace.prefix()]);
        let once_key = once_key.map(|key| self.namespace.prefix_key(key));
        self.txn
            .append_shared(&log, entry, once_key.as_deref())
            .await
    }

    async fn set_shared(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let prefixed = self.namespace.prefix_key(key);
        self.txn.set_shared(&prefixed, value).await
    }

    async fn delete_shared(&self, key: &[u8]) -> Result<()> {
        let prefixed = self.namespace.prefix_key(key);
        self.txn.delete_shared(&prefixed).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Txn for NamespacedTxn {
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

/// A namespaced iterator that strips prefixes from returned keys
pub struct NamespacedIterator {
    iter: Box<dyn Iterator>,
    namespace: Namespace,
}

impl crate::corekv::private::Sealed for NamespacedIterator {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Iterator for NamespacedIterator {
    async fn next(&mut self) -> Result<Option<KvPair>> {
        match self.iter.next().await? {
            Some(pair) => {
                // Strip the namespace prefix from the key
                let unprefixed_key = self.namespace.unprefix_key(&pair.key)?;
                Ok(Some(KvPair {
                    key: unprefixed_key.to_vec(),
                    value: pair.value,
                }))
            }
            None => Ok(None),
        }
    }

    async fn close(&mut self) -> Result<()> {
        self.iter.close().await
    }

    async fn seek(&mut self, key: &[u8]) -> Result<bool> {
        // Prefix the key before seeking in the underlying iterator
        let prefixed_key = self.namespace.prefix_key(key);
        self.iter.seek(&prefixed_key).await
    }

    async fn reset(&mut self) -> Result<()> {
        self.iter.reset().await
    }

    fn is_valid(&self) -> bool {
        self.iter.is_valid()
    }
}

// Tests extracted to crates/storage/tests/namespace_tests.rs
