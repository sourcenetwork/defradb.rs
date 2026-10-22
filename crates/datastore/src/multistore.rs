/// Transaction-aware Multistore for the datastore layer.
///
/// Unlike the storage layer's Multistore which provides store-level access,
/// this provides namespaced access to a single underlying transaction.
/// This matches Go DefraDB's internal/datastore/multi.go pattern.
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;
use storage::corekv::{
    Error, IterOptions, Iterator, OnCommit, QueuedCallbacks, Reader, Result, Txn, Writer,
};
use storage::namespace::Namespace;

/// One transaction shared by its namespace views.
///
/// Reads and writes take `&self` and go straight to the transaction, which
/// takes them that way: views share it with no lock. A callback registered
/// through a view waits in a queue, in order ([`QueuedCallbacks`]), until it
/// is settled into the transaction. A savepoint takes `&mut self` and turning
/// it back into the transaction takes `self`; only the holder of the one
/// remaining reference gets either ([`Arc::get_mut`], [`Arc::try_unwrap`]), so
/// a view still open at that moment is an error, never a wait. Each settles
/// the queue first, so a savepoint's rollback takes back exactly what was
/// registered after it, and so does a drop, so a transaction that ends with
/// its last view still runs the abort callbacks registered through views.
pub struct SharedTxn {
    /// `None` only once [`into_txn`](Self::into_txn) has taken it.
    txn: Option<Box<dyn Txn>>,
    queued: QueuedCallbacks,
    /// The CRDT types of the node whose store the transaction runs on.
    crdts: storage::Crdts,
}

impl SharedTxn {
    fn txn(&self) -> Result<&dyn Txn> {
        self.txn.as_deref().ok_or(Error::DiscardedTxn)
    }

    fn txn_mut(&mut self) -> Result<&mut Box<dyn Txn>> {
        self.txn.as_mut().ok_or(Error::DiscardedTxn)
    }

    pub(crate) fn set_savepoint(&mut self) -> Result<()> {
        self.settle()?;
        self.txn_mut()?.set_savepoint()
    }

    pub(crate) fn rollback_to_savepoint(&mut self) -> Result<()> {
        self.settle()?;
        self.txn_mut()?.rollback_to_savepoint()
    }

    pub(crate) fn release_savepoint(&mut self) -> Result<()> {
        self.settle()?;
        self.txn_mut()?.release_savepoint()
    }

    /// Hands the callbacks registered through views to the transaction, in
    /// order.
    fn settle(&mut self) -> Result<()> {
        let Some(txn) = self.txn.as_mut() else {
            return Ok(());
        };
        self.queued.settle_into(txn)
    }

    /// Where callbacks registered through a shared handle wait.
    pub(crate) fn queued(&self) -> &QueuedCallbacks {
        &self.queued
    }

    pub(crate) fn try_into_txn(shared: Arc<Self>) -> Result<Box<dyn Txn>> {
        let mut shared = Arc::try_unwrap(shared)
            .map_err(|_| Error::Other("transaction still has namespace references".into()))?;
        shared.settle()?;
        shared.txn.take().ok_or(Error::DiscardedTxn)
    }

    pub async fn merge(&self, namespace: Namespace, key: &[u8], operand: &[u8]) -> Result<()> {
        self.txn()?
            .merge_shared(&prefix_key(namespace, key), operand)
            .await
    }

    pub async fn append(
        &self,
        namespace: Namespace,
        log: &storage::corekv::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> Result<()> {
        let log = log.prefixed(&[namespace.prefix()]);
        let once_key = once_key.map(|key| prefix_key(namespace, key));
        self.txn()?
            .append_shared(&log, entry, once_key.as_deref())
            .await
    }

    /// Create a new shared transaction.
    // `Txn` drops its `Send + Sync` bounds on wasm32 (see `MaybeSendSync`), so
    // there the Arc wraps a non-Send value. That is sound in the
    // single-threaded browser runtime, and the return type has to stay `Arc`
    // for native callers that share it across threads.
    #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
    pub fn new(txn: Box<dyn Txn>, crdts: storage::Crdts) -> Arc<Self> {
        Arc::new(Self {
            queued: QueuedCallbacks::new(txn.is_readonly()),
            txn: Some(txn),
            crdts,
        })
    }

    /// The CRDT types of the node whose store the transaction runs on.
    pub fn crdts(&self) -> &storage::Crdts {
        &self.crdts
    }

    /// Get a value with namespace prefix.
    pub async fn get(&self, namespace: Namespace, key: &[u8]) -> Result<Option<Bytes>> {
        self.txn()?.get(&prefix_key(namespace, key)).await
    }

    /// Read selected logical parts through the shared engine transaction.
    pub async fn get_parts(
        &self,
        namespace: Namespace,
        key: &[u8],
        parts: &[u32],
    ) -> Result<Option<Bytes>> {
        self.txn()?
            .get_parts(&prefix_key(namespace, key), parts)
            .await
    }

    /// Check if a key exists with namespace prefix.
    pub async fn has(&self, namespace: Namespace, key: &[u8]) -> Result<bool> {
        self.txn()?.has(&prefix_key(namespace, key)).await
    }

    /// Get value size with namespace prefix.
    pub async fn get_size(&self, namespace: Namespace, key: &[u8]) -> Result<Option<usize>> {
        self.txn()?.get_size(&prefix_key(namespace, key)).await
    }

    /// Set a value with namespace prefix.
    pub async fn set(&self, namespace: Namespace, key: &[u8], value: &[u8]) -> Result<()> {
        self.txn()?
            .set_shared(&prefix_key(namespace, key), value)
            .await
    }

    /// Delete a key with namespace prefix.
    pub async fn delete(&self, namespace: Namespace, key: &[u8]) -> Result<()> {
        self.txn()?.delete_shared(&prefix_key(namespace, key)).await
    }

    /// Create an iterator with namespace prefix.
    pub async fn iterator(
        &self,
        namespace: Namespace,
        opts: IterOptions,
    ) -> Result<Box<dyn Iterator>> {
        let iter = self
            .txn()?
            .iterator(prefix_iter_options(namespace, opts))
            .await?;
        Ok(Box::new(NamespacedIterator { iter, namespace }))
    }

    /// Get from rootstore (no namespace prefix).
    pub async fn root_get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.txn()?.get(key).await
    }

    /// Set in rootstore (no namespace prefix).
    pub async fn root_set(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.txn()?.set_shared(key, value).await
    }

    /// Delete from rootstore (no namespace prefix).
    pub async fn root_delete(&self, key: &[u8]) -> Result<()> {
        self.txn()?.delete_shared(key).await
    }

    /// Register `callback` to run once if the transaction commits. A
    /// read-only transaction refuses ordered work.
    pub async fn on_commit(&self, callback: OnCommit) -> Result<()> {
        self.queued.on_commit(callback)
    }

    /// Check if readonly.
    pub async fn is_readonly(&self) -> bool {
        self.txn.as_ref().is_none_or(|txn| txn.is_readonly())
    }

    /// Consume self and return the underlying transaction, with the callbacks
    /// registered through views settled into it.
    pub fn into_txn(mut self) -> Box<dyn Txn> {
        if let Err(error) = self.settle() {
            // Unreachable: each callback was checked against this
            // transaction when it was registered.
            tracing::error!(%error, "a callback registered through a view was refused");
        }
        match self.txn.take() {
            Some(txn) => txn,
            // `txn` is taken only here, and this consumes `self`.
            None => unreachable!("a shared transaction is turned back into its transaction once"),
        }
    }
}

impl Drop for SharedTxn {
    /// Ending with its last view, the transaction still gets the callbacks
    /// registered through views: its abort callbacks run when it drops.
    fn drop(&mut self) {
        if let Err(error) = self.settle() {
            tracing::error!(%error, "a callback registered through a view was refused");
        }
    }
}

/// A [`NamespaceView`] that does not keep its transaction in use.
#[derive(Clone)]
pub struct WeakNamespaceView {
    txn: std::sync::Weak<SharedTxn>,
    namespace: Namespace,
}

impl WeakNamespaceView {
    /// The view, while its transaction has not been committed, rolled back or
    /// dropped.
    pub fn upgrade(&self) -> Option<NamespaceView> {
        self.txn.upgrade().map(|txn| NamespaceView {
            txn,
            namespace: self.namespace,
        })
    }
}

/// A view into a specific namespace of a shared transaction.
pub struct NamespaceView {
    txn: Arc<SharedTxn>,
    namespace: Namespace,
}

impl NamespaceView {
    pub async fn merge(&self, key: &[u8], operand: &[u8]) -> Result<()> {
        self.txn.merge(self.namespace, key, operand).await
    }

    pub async fn append(
        &self,
        log: &storage::corekv::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> Result<()> {
        self.txn.append(self.namespace, log, entry, once_key).await
    }

    /// Create a new namespace view.
    pub fn new(txn: Arc<SharedTxn>, namespace: Namespace) -> Self {
        Self { txn, namespace }
    }

    /// The CRDT types of the node whose store the transaction runs on.
    pub fn crdts(&self) -> &storage::Crdts {
        self.txn.crdts()
    }

    /// View a different namespace of the same transaction.
    ///
    /// Lets a caller holding one view reach a sibling namespace without
    /// threading the transaction (or a second view) through its signature.
    pub fn sibling(&self, namespace: Namespace) -> Self {
        Self {
            txn: self.txn.clone(),
            namespace,
        }
    }

    /// A handle to this view that does not keep the transaction in use, so
    /// the transaction can still commit or roll back while it is held.
    pub fn downgrade(&self) -> WeakNamespaceView {
        WeakNamespaceView {
            txn: Arc::downgrade(&self.txn),
            namespace: self.namespace,
        }
    }

    /// Get a value.
    pub async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.txn.get(self.namespace, key).await
    }

    /// Read logical parts while retaining this view's namespace.
    pub async fn get_parts(&self, key: &[u8], parts: &[u32]) -> Result<Option<Bytes>> {
        self.txn.get_parts(self.namespace, key, parts).await
    }

    /// Check if a key exists.
    pub async fn has(&self, key: &[u8]) -> Result<bool> {
        self.txn.has(self.namespace, key).await
    }

    /// Get value size.
    pub async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        self.txn.get_size(self.namespace, key).await
    }

    /// Set a value.
    pub async fn set(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.txn.set(self.namespace, key, value).await
    }

    /// Delete a key.
    pub async fn delete(&self, key: &[u8]) -> Result<()> {
        self.txn.delete(self.namespace, key).await
    }

    /// Create an iterator.
    pub async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>> {
        self.txn.iterator(self.namespace, opts).await
    }

    /// Register `callback` to run once if the shared transaction commits.
    pub async fn on_commit(&self, callback: OnCommit) -> Result<()> {
        self.txn.on_commit(callback).await
    }
}

impl Clone for NamespaceView {
    fn clone(&self) -> Self {
        Self {
            txn: self.txn.clone(),
            namespace: self.namespace,
        }
    }
}

impl storage::corekv::private::Sealed for NamespaceView {}

/// Implement Reader trait for NamespaceView to enable index operations.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Reader for NamespaceView {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.txn.get(self.namespace, key).await
    }

    async fn get_parts(&self, key: &[u8], parts: &[u32]) -> Result<Option<Bytes>> {
        self.txn.get_parts(self.namespace, key, parts).await
    }

    async fn has(&self, key: &[u8]) -> Result<bool> {
        self.txn.has(self.namespace, key).await
    }

    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        self.txn.get_size(self.namespace, key).await
    }

    async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>> {
        self.txn.iterator(self.namespace, opts).await
    }
}

/// Implement Writer trait for NamespaceView to enable index operations.
///
/// Writes go through the shared transaction, which takes them by `&self`.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Writer for NamespaceView {
    async fn merge_shared(&self, key: &[u8], operand: &[u8]) -> Result<()> {
        self.txn.merge(self.namespace, key, operand).await
    }

    async fn append_shared(
        &self,
        log: &storage::corekv::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> Result<()> {
        self.txn.append(self.namespace, log, entry, once_key).await
    }

    async fn set_shared(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.txn.set(self.namespace, key, value).await
    }

    async fn delete_shared(&self, key: &[u8]) -> Result<()> {
        self.txn.delete(self.namespace, key).await
    }
}

/// A view into the rootstore (no namespace prefix).
pub struct RootView {
    txn: Arc<SharedTxn>,
}

impl RootView {
    /// Create a new root view.
    pub fn new(txn: Arc<SharedTxn>) -> Self {
        Self { txn }
    }

    /// Get a value.
    pub async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.txn.root_get(key).await
    }

    /// Set a value.
    pub async fn set(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.txn.root_set(key, value).await
    }

    /// Delete a key.
    pub async fn delete(&self, key: &[u8]) -> Result<()> {
        self.txn.root_delete(key).await
    }
}

/// Helper to prefix a key with namespace.
fn prefix_key(namespace: Namespace, key: &[u8]) -> Vec<u8> {
    let mut prefixed = Vec::with_capacity(1 + key.len());
    prefixed.push(namespace.prefix());
    prefixed.extend_from_slice(key);
    prefixed
}

/// Helper to unprefix a key.
fn unprefix_key(namespace: Namespace, key: &[u8]) -> Result<Vec<u8>> {
    if key.is_empty() {
        return Err(Error::EmptyKey);
    }
    if key[0] != namespace.prefix() {
        return Err(Error::Other(format!(
            "Key has wrong prefix: expected {:?} (0x{:02x}), got 0x{:02x}",
            namespace.prefix() as char,
            namespace.prefix(),
            key[0]
        )));
    }
    Ok(key[1..].to_vec())
}

/// Helper to prefix iterator options.
fn prefix_iter_options(namespace: Namespace, opts: IterOptions) -> IterOptions {
    let unbounded = opts.prefix().is_none() && opts.start().is_none() && opts.end().is_none();
    let prefixed_opts = opts.map_keys(|key| prefix_key(namespace, key));
    if unbounded {
        prefixed_opts.with_prefix(vec![namespace.prefix()])
    } else {
        prefixed_opts
    }
}

/// Iterator that strips namespace prefix from keys.
struct NamespacedIterator {
    iter: Box<dyn Iterator>,
    namespace: Namespace,
}

impl storage::corekv::private::Sealed for NamespacedIterator {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Iterator for NamespacedIterator {
    async fn next(&mut self) -> Result<Option<storage::corekv::KvPair>> {
        match self.iter.next().await? {
            Some(pair) => {
                let unprefixed_key = unprefix_key(self.namespace, &pair.key)?;
                Ok(Some(storage::corekv::KvPair {
                    key: unprefixed_key,
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
        let prefixed_key = prefix_key(self.namespace, key);
        self.iter.seek(&prefixed_key).await
    }

    async fn reset(&mut self) -> Result<()> {
        self.iter.reset().await
    }

    fn is_valid(&self) -> bool {
        self.iter.is_valid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use storage::corekv::Store;
    use storage::RegolithStore;

    #[test]
    fn test_shared_txn_namespace_isolation() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let txn = store.new_txn(false).await.unwrap();
            let shared = SharedTxn::new(txn, storage::Crdts::default());

            // Write to datastore namespace
            shared
                .set(Namespace::Datastore, b"key", b"datastore_value")
                .await
                .unwrap();

            // Write to systemstore namespace
            shared
                .set(Namespace::Systemstore, b"key", b"systemstore_value")
                .await
                .unwrap();

            // Read back - each should see its own value
            assert_eq!(
                shared.get(Namespace::Datastore, b"key").await.unwrap(),
                Some(Bytes::from_static(b"datastore_value"))
            );
            assert_eq!(
                shared.get(Namespace::Systemstore, b"key").await.unwrap(),
                Some(Bytes::from_static(b"systemstore_value"))
            );
        })
        .unwrap()
    }

    #[test]
    fn test_namespace_view() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let txn = store.new_txn(false).await.unwrap();
            let shared = SharedTxn::new(txn, storage::Crdts::default());

            let ds = NamespaceView::new(shared.clone(), Namespace::Datastore);
            let ss = NamespaceView::new(shared.clone(), Namespace::Systemstore);

            // Write through views
            ds.set(b"key", b"datastore_value").await.unwrap();
            ss.set(b"key", b"systemstore_value").await.unwrap();

            // Read back
            assert_eq!(
                ds.get(b"key").await.unwrap(),
                Some(Bytes::from_static(b"datastore_value"))
            );
            assert_eq!(
                ss.get(b"key").await.unwrap(),
                Some(Bytes::from_static(b"systemstore_value"))
            );
        })
        .unwrap()
    }

    #[test]
    fn test_root_view_sees_prefixed() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let txn = store.new_txn(false).await.unwrap();
            let shared = SharedTxn::new(txn, storage::Crdts::default());

            let ds = NamespaceView::new(shared.clone(), Namespace::Datastore);
            let root = RootView::new(shared.clone());

            // Write through datastore view
            ds.set(b"mykey", b"value").await.unwrap();

            // Root should see it with 'd' prefix
            let value = root.get(b"dmykey").await.unwrap();
            assert_eq!(value, Some(Bytes::from_static(b"value")));
        })
        .unwrap()
    }

    #[test]
    fn test_namespace_iterator() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let txn = store.new_txn(false).await.unwrap();
            let shared = SharedTxn::new(txn, storage::Crdts::default());

            let ds = NamespaceView::new(shared.clone(), Namespace::Datastore);

            // Write multiple keys
            ds.set(b"key1", b"value1").await.unwrap();
            ds.set(b"key2", b"value2").await.unwrap();
            ds.set(b"key3", b"value3").await.unwrap();

            // Iterate
            let mut iter = ds.iterator(IterOptions::new()).await.unwrap();
            let mut count = 0;
            while let Some(pair) = iter.next().await.unwrap() {
                assert!(pair.key.starts_with(b"key"));
                count += 1;
            }
            assert_eq!(count, 3);
        })
        .unwrap()
    }
}
