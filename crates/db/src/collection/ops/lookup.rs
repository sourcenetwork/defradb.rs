use super::*;
use crate::collection::Cached;

impl<S: Store> crate::database::DB<S> {
    /// List all collection names using the transaction's cache.
    ///
    /// This loads all collections from the store into the transaction cache
    /// if they haven't been loaded yet.
    pub async fn list_collections_with_txn(&self, txn: &mut DbTxn<S>) -> Result<Vec<String>> {
        txn.load_all_collections().await?;
        Ok(txn.collection_cache().names())
    }

    /// List all collection names.
    ///
    /// Uses the process-wide cache. For transaction-scoped access, use `list_collections_with_txn`.
    pub fn list_collections(&self) -> Result<Vec<String>> {
        Ok(self
            .collections
            .peek(|cache| cache.names().cloned().collect()))
    }

    /// Cache `schema` under its name, reporting whether the cache took it.
    ///
    /// Used by the merge handler to make a collection synced over p2p visible
    /// to `list_collections` and `get_collection`. Such a collection can be
    /// inactive: a synced one starts inactive until it is activated.
    ///
    /// [`Cached::NameHeldByAnother`] means an entry naming a different
    /// collection already holds the name and was left alone, so the cache is
    /// unchanged.
    /// A `Collection` whose write/query index sets reflect the persisted
    /// action statuses. Every cache refresh path goes through this, so a
    /// cached entry never silently widens an ERRORED index back into writes
    /// or an in-progress one into queries.
    pub(crate) async fn collection_with_index_actions(
        &self,
        schema: CollectionVersion,
    ) -> Result<Collection> {
        let txn = self.new_txn(true).await?;
        let result = Collection::load_index_actions(schema, &txn.systemstore()?).await;
        let _ = txn.discard();
        result
    }

    pub async fn add_collection_to_cache(&self, schema: CollectionVersion) -> Result<Cached> {
        let name = schema.name.clone();
        let collection = self.collection_with_index_actions(schema.clone()).await?;

        let offered = schema.collection_id.clone();
        let mut cached = Cached::Taken;
        self.collections.rcu(|old| {
            let mut cache = old.clone();
            cached = cache.offer(collection.clone());
            cache
        });
        if cached == Cached::NameHeldByAnother {
            tracing::warn!(
                collection_name = %name,
                held_by_name = "another collection id",
                %offered,
                "Refusing to displace a cached collection with a different collection ID"
            );
        }
        Ok(cached)
    }

    /// Get a collection by name using the transaction's cache.
    ///
    /// This performs lazy loading - the collection is loaded from the store
    /// on first access within the transaction.
    pub async fn get_collection_with_txn(
        &self,
        txn: &mut DbTxn<S>,
        name: &str,
    ) -> Result<Option<Collection>> {
        txn.get_collection(name).await.map(|opt| opt.cloned())
    }

    /// Get a collection by name.
    ///
    /// Uses the process-wide cache. For transaction-scoped access, use `get_collection_with_txn`.
    pub fn get_collection(&self, name: &str) -> Result<Option<Collection>> {
        Ok(self.collections.peek(|cache| cache.get(name).cloned()))
    }

    /// Check if a collection exists using the transaction's cache.
    ///
    /// This performs lazy loading - the collection is loaded from the store
    /// on first access within the transaction.
    pub async fn has_collection_with_txn(&self, txn: &mut DbTxn<S>, name: &str) -> Result<bool> {
        Ok(txn.get_collection(name).await?.is_some())
    }

    /// Check if a collection exists.
    ///
    /// Uses the process-wide cache. For transaction-scoped access, use `has_collection_with_txn`.
    pub fn has_collection(&self, name: &str) -> Result<bool> {
        Ok(self.collections.peek(|cache| cache.contains_name(name)))
    }

    /// Find a collection by its collection ID (schema version ID).
    ///
    /// This is useful for P2P sync where we receive blocks with schema_version_id
    /// and need to find the corresponding collection.
    ///
    /// Uses the process-wide cache.
    pub fn find_collection_by_id(&self, collection_id: &str) -> Result<Option<Collection>> {
        Ok(self
            .collections
            .peek(|cache| cache.by_id(collection_id).cloned()))
    }

    pub(crate) fn forbid_collection_id(&self, collection_id: &str) -> Result<()> {
        self.forbidden_collection_ids
            .insert(collection_id.to_string(), ());
        Ok(())
    }

    pub(crate) fn unforbid_collection_id(&self, collection_id: &str) -> Result<()> {
        self.forbidden_collection_ids.remove(collection_id);
        Ok(())
    }

    pub(crate) fn is_collection_forbidden(&self, collection_id: &str) -> Result<bool> {
        Ok(self.forbidden_collection_ids.contains_key(collection_id))
    }

    /// Get a snapshot of all collections (for use by DbTransactionRegistry).
    ///
    /// Returns an immutable snapshot that provides snapshot isolation for transactions.
    pub fn collections_snapshot(&self) -> Result<CollectionSnapshot> {
        Ok(CollectionSnapshot::new(
            self.collections.load_clone().by_name(),
        ))
    }
}
