//! The durable set of P2P collection subscriptions: the desired state the
//! live collection topics converge to.
//!
//! Mirrors Go DefraDB's systemstore `/p2p/collection/` key structure. Each
//! change to the set is one engine transaction under the engine retry, so a
//! subscribe and an unsubscribe of one collection that race settle in the
//! engine, the later commit winning, with no lock around them.

use crate::error::{Error, Result};
use async_trait::async_trait;
use datastore::BasicTxn;
use defra_core::thread_bounds::MaybeSendSync;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use std::sync::Arc;
use storage::corekv::{
    IterOptions, Key, Reader, RetryAttemptError, RetryAttemptValue, RetryOptions, Store,
};
use storage::keys::systemstore::P2PCollectionKey;
use storage::stores::Systemstore;

/// Marker byte for collection subscription (matches Go's marker = byte(0xff))
const COLLECTION_MARKER: u8 = 0xff;

/// The subscribed set of collections, as desired state.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait P2PCollectionStorage: MaybeSendSync {
    /// Subscribe every collection not subscribed yet, atomically, and return
    /// how many that was.
    async fn add_collections(&self, collection_ids: &[String]) -> Result<usize>;

    /// Unsubscribe every collection, atomically.
    async fn remove_collections(&self, collection_ids: &[String]) -> Result<()>;

    /// Every subscribed collection ID.
    async fn get_all_collections(&self) -> Result<Vec<String>>;

    /// Whether a collection is subscribed.
    async fn is_subscribed(&self, collection_id: &str) -> Result<bool>;
}

/// The subscribed set in the systemstore.
pub struct P2PCollectionStore<S: Store> {
    store: Arc<S>,
    systemstore: Systemstore<S>,
    retry: RetryOptions,
}

impl<S: Store> P2PCollectionStore<S> {
    /// Create a new P2P collection store.
    pub fn new(store: Arc<S>) -> Self {
        Self {
            systemstore: Systemstore::new(Arc::clone(&store)),
            store,
            retry: storage::corekv::engine_retry_options(storage::corekv::DEFAULT_MAX_TXN_RETRIES),
        }
    }

    /// Re-run a conflicting change up to `max_txn_retries` times, the node's
    /// `datastore.max_txn_retries`.
    pub fn with_max_txn_retries(mut self, max_txn_retries: u32) -> Self {
        self.retry = storage::corekv::engine_retry_options(max_txn_retries);
        self
    }

    /// Runs `change` as one engine transaction and returns how many
    /// collections it subscribed.
    async fn change(&self, change: SubscriptionChange) -> Result<usize> {
        datastore::transact(&*self.store, self.retry, move |txn| {
            Box::pin(change.clone().apply(txn))
        })
        .await
        .map_err(|error| Error::Storage(error.into_storage().to_string()))
    }
}

/// One change to the subscribed set.
#[derive(Clone)]
enum SubscriptionChange {
    /// Marks the keys not marked yet: each is read, so a concurrent
    /// unsubscribe of the same collection makes the attempt run again.
    Add(Vec<Vec<u8>>),
    /// Deletes the keys' marks.
    Remove(Vec<Vec<u8>>),
}

impl SubscriptionChange {
    fn keys(collection_ids: &[String]) -> Vec<Vec<u8>> {
        collection_ids
            .iter()
            .map(|collection_id| P2PCollectionKey::new(collection_id).bytes())
            .collect()
    }

    /// Applies the change in one attempt; its value is how many collections
    /// it subscribed.
    async fn apply(
        self,
        txn: &mut BasicTxn,
    ) -> std::result::Result<RetryAttemptValue, RetryAttemptError> {
        let systemstore = txn.systemstore();
        let mut added = 0usize;
        match self {
            Self::Add(keys) => {
                for key in &keys {
                    if systemstore.get(key).await?.is_none() {
                        systemstore.set(key, &[COLLECTION_MARKER]).await?;
                        added += 1;
                    }
                }
            }
            Self::Remove(keys) => {
                for key in &keys {
                    systemstore.delete(key).await?;
                }
            }
        }
        Ok(Box::new(added) as RetryAttemptValue)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static> P2PCollectionStorage for P2PCollectionStore<S> {
    async fn add_collections(&self, collection_ids: &[String]) -> Result<usize> {
        if collection_ids.is_empty() {
            return Ok(0);
        }
        self.change(SubscriptionChange::Add(SubscriptionChange::keys(
            collection_ids,
        )))
        .await
    }

    async fn remove_collections(&self, collection_ids: &[String]) -> Result<()> {
        if collection_ids.is_empty() {
            return Ok(());
        }
        self.change(SubscriptionChange::Remove(SubscriptionChange::keys(
            collection_ids,
        )))
        .await
        .map(|_| ())
    }

    async fn get_all_collections(&self) -> Result<Vec<String>> {
        let txn = self
            .systemstore
            .new_txn(true)
            .await
            .map_err(|e| Error::Storage(e.to_string()))?;

        let mut collections = Vec::new();

        // Iterate over all keys with the P2P collection prefix
        let opts = IterOptions::new().with_prefix(P2PCollectionKey::p2p_collection_prefix());
        let mut iter = txn
            .iterator(opts)
            .await
            .map_err(|e| Error::Storage(e.to_string()))?;

        while let Some(kv) = iter
            .next()
            .await
            .map_err(|e| Error::Storage(e.to_string()))?
        {
            if let Some(collection_id) = parse_collection_id(&kv.key) {
                collections.push(collection_id);
            }
        }

        iter.close()
            .await
            .map_err(|e| Error::Storage(e.to_string()))?;

        Ok(collections)
    }

    async fn is_subscribed(&self, collection_id: &str) -> Result<bool> {
        let key = P2PCollectionKey::new(collection_id);
        let txn = self
            .systemstore
            .new_txn(true)
            .await
            .map_err(|e| Error::Storage(e.to_string()))?;

        let value = txn
            .get(&key.bytes())
            .await
            .map_err(|e| Error::Storage(e.to_string()))?;

        Ok(value.is_some())
    }
}

fn parse_collection_id(key: &[u8]) -> Option<String> {
    let prefix = P2PCollectionKey::p2p_collection_prefix();
    key.strip_prefix(prefix.as_slice())
        .and_then(|id_bytes| String::from_utf8(id_bytes.to_vec()).ok())
        .filter(|id| !id.is_empty())
}

/// The subscribed set kept in process memory, for a node with no store:
/// nothing outlives the process.
pub struct MemoryCollectionStorage {
    collections: HopscotchMap<String, (), RandomState>,
}

impl Default for MemoryCollectionStorage {
    fn default() -> Self {
        Self {
            collections: HopscotchMap::with_hasher(RandomState::default()),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl P2PCollectionStorage for MemoryCollectionStorage {
    async fn add_collections(&self, collection_ids: &[String]) -> Result<usize> {
        Ok(collection_ids
            .iter()
            .filter(|collection_id| {
                self.collections
                    .insert_if_absent((*collection_id).clone(), ())
                    .is_none()
            })
            .count())
    }

    async fn remove_collections(&self, collection_ids: &[String]) -> Result<()> {
        for collection_id in collection_ids {
            self.collections.remove(collection_id);
        }
        Ok(())
    }

    async fn get_all_collections(&self) -> Result<Vec<String>> {
        Ok(self.collections.keys().collect())
    }

    async fn is_subscribed(&self, collection_id: &str) -> Result<bool> {
        Ok(self.collections.contains_key(collection_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use storage::RegolithStore;

    fn collections(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_string()).collect()
    }

    #[test]
    fn test_p2p_collection_key() {
        let key = P2PCollectionKey::new("users");
        assert_eq!(key.bytes(), b"/p2p/collection/users");
        assert_eq!(key.to_string(), "/p2p/collection/users");
    }

    #[test]
    fn test_parse_collection_id() {
        let id = parse_collection_id(b"/p2p/collection/users");
        assert_eq!(id, Some("users".to_string()));

        let id = parse_collection_id(b"/other/key");
        assert_eq!(id, None);
    }

    #[nuclei::test]
    async fn stores_collection_state_in_systemstore_namespace() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let p2p_store = P2PCollectionStore::new(store.clone());

        assert_eq!(
            p2p_store
                .add_collections(&collections(&["users"]))
                .await
                .unwrap(),
            1
        );

        let systemstore = Systemstore::new(store.clone());
        let system_txn = systemstore.new_txn(true).await.unwrap();
        assert_eq!(
            system_txn
                .get(&P2PCollectionKey::new("users").bytes())
                .await
                .unwrap(),
            Some(Bytes::from(vec![COLLECTION_MARKER]))
        );

        let root_txn = store.new_txn(true).await.unwrap();
        assert_eq!(
            root_txn
                .get(&P2PCollectionKey::new("users").bytes())
                .await
                .unwrap(),
            None,
            "Go-compatible P2P collection state must live in Systemstore, not the raw root store"
        );
    }

    #[nuclei::test]
    async fn stores_collection_batch_in_systemstore_namespace() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let p2p_store = P2PCollectionStore::new(store);
        let collections = collections(&["users", "messages", "sessions"]);

        assert_eq!(p2p_store.add_collections(&collections).await.unwrap(), 3);

        let stored = p2p_store
            .get_all_collections()
            .await
            .unwrap()
            .into_iter()
            .collect::<rapidhash::RapidHashSet<_>>();
        assert_eq!(stored, collections.into_iter().collect());
    }

    #[nuclei::test]
    async fn adding_counts_only_the_collections_not_subscribed_yet() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let p2p_store = P2PCollectionStore::new(store);

        assert_eq!(
            p2p_store
                .add_collections(&collections(&["users"]))
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            p2p_store
                .add_collections(&collections(&["users", "messages"]))
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            p2p_store
                .add_collections(&collections(&["users", "messages"]))
                .await
                .unwrap(),
            0
        );
        assert_eq!(p2p_store.add_collections(&[]).await.unwrap(), 0);
    }

    #[nuclei::test]
    async fn removes_collection_batch_from_systemstore_namespace() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let p2p_store = P2PCollectionStore::new(store);
        let collections = collections(&["users", "messages", "sessions"]);
        p2p_store.add_collections(&collections).await.unwrap();

        p2p_store
            .remove_collections(&collections[..2])
            .await
            .unwrap();

        assert_eq!(
            p2p_store.get_all_collections().await.unwrap(),
            vec!["sessions".to_string()]
        );
        p2p_store
            .remove_collections(&collections[..2])
            .await
            .expect("removing collections already gone is a no-op");
    }

    /// The change is one engine transaction: concurrent subscribes and
    /// unsubscribes of the same collections all commit, through the engine
    /// retry where they race, and leave the state of the last one.
    #[nuclei::test(threads = 4)]
    async fn racing_subscribes_and_unsubscribes_all_commit() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let p2p_store = Arc::new(P2PCollectionStore::new(store).with_max_txn_retries(64));
        let collections = collections(&["users", "messages"]);
        let mut tasks = Vec::new();
        for index in 0..16 {
            let p2p_store = Arc::clone(&p2p_store);
            let collections = collections.clone();
            tasks.push(crate::rt::spawn(async move {
                if index % 2 == 0 {
                    p2p_store.add_collections(&collections).await.map(|_| ())
                } else {
                    p2p_store.remove_collections(&collections).await
                }
            }));
        }
        for task in tasks {
            task.join().await.unwrap().unwrap();
        }

        p2p_store.add_collections(&collections).await.unwrap();
        let stored = p2p_store
            .get_all_collections()
            .await
            .unwrap()
            .into_iter()
            .collect::<rapidhash::RapidHashSet<_>>();
        assert_eq!(stored, collections.into_iter().collect());
    }

    #[nuclei::test]
    async fn loads_collection_state_written_like_go_systemstore() {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let systemstore = Systemstore::new(store.clone());
        let mut system_txn = systemstore.new_txn(false).await.unwrap();
        system_txn
            .set(
                &P2PCollectionKey::new("users").bytes(),
                &[COLLECTION_MARKER],
            )
            .await
            .unwrap();
        system_txn.commit().await.unwrap();

        let p2p_store = P2PCollectionStore::new(store);

        assert!(p2p_store.is_subscribed("users").await.unwrap());
        assert_eq!(
            p2p_store.get_all_collections().await.unwrap(),
            vec!["users".to_string()]
        );
    }

    #[nuclei::test]
    async fn memory_storage_keeps_the_subscribed_set_in_process() {
        let storage = MemoryCollectionStorage::default();
        assert_eq!(
            storage
                .add_collections(&collections(&["users", "users"]))
                .await
                .unwrap(),
            1
        );
        assert!(storage.is_subscribed("users").await.unwrap());
        assert_eq!(
            storage.get_all_collections().await.unwrap(),
            vec!["users".to_string()]
        );
        storage
            .remove_collections(&collections(&["users"]))
            .await
            .unwrap();
        assert!(!storage.is_subscribed("users").await.unwrap());
    }
}
