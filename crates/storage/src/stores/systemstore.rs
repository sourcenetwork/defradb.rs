use crate::corekv::{Result, Store, Txn};
use crate::namespace::{Namespace, NamespacedStore};
use async_trait::async_trait;
use std::sync::Arc;

/// Systemstore provides storage for metadata and configuration
pub struct Systemstore<S: Store> {
    store: NamespacedStore<S>,
}

impl<S: Store> Systemstore<S> {
    /// Create a new Systemstore
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store: NamespacedStore::new(store, Namespace::Systemstore),
        }
    }
}

impl<S: Store> crate::corekv::private::Sealed for Systemstore<S> {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store> Store for Systemstore<S> {
    async fn allocate(&self, key: &[u8], count: u64) -> Result<std::ops::Range<u64>> {
        self.store.allocate(key, count).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn transaction_stats_handle(&self) -> Option<crate::backends::TransactionStatsHandle> {
        self.store.transaction_stats_handle()
    }

    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>> {
        self.store.new_txn(readonly).await
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::RegolithStore;
    use crate::corekv::Key;
    use crate::keys::systemstore::CollectionKey;
    use bytes::Bytes;

    #[test]
    fn test_systemstore_basic() {
        workers::block_on(async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let systemstore = Systemstore::new(store);

            let key = CollectionKey::new("users_v1");

            // Write
            let mut txn = systemstore.new_txn(false).await.unwrap();
            txn.set(&key.bytes(), b"collection_definition")
                .await
                .unwrap();
            txn.commit().await.unwrap();

            // Read
            let txn = systemstore.new_txn(true).await.unwrap();
            let value = txn.get(&key.bytes()).await.unwrap();
            assert_eq!(value, Some(Bytes::from_static(b"collection_definition")));
        })
        .unwrap()
    }
}
