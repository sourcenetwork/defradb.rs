use super::*;

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
            .peek(|cache| cache.keys().cloned().collect()))
    }

    /// Add a collection to the runtime cache.
    ///
    /// This is used by the merge handler to add synced collections received via P2P
    /// to the cache so they're visible to `list_collections` and `get_collection`.
    /// The collection can be inactive (synced collections start inactive until manually activated).
    /// Cache `schema` under its name, returning whether it was taken.
    ///
    /// `false` means an entry naming a different collection already holds the
    /// name and was left alone; the caller's schema is unchanged in the cache.
    pub fn add_collection_to_cache(&self, schema: CollectionVersion) -> Result<bool> {
        let name = schema.name.clone();
        // The cache is keyed by name, but a collection's identity is its
        // collection ID. An entry naming a different collection must not be
        // replaced: whatever that collection knows and the incoming schema
        // does not carry would be dropped silently. A placeholder is a
        // stand-in for a definition that has not arrived, so it always yields.
        let mut displaced = true;
        self.collections.rcu(|old| {
            if let Some(existing) = old.get(&name) {
                let existing = existing.schema();
                if existing.collection_id != schema.collection_id && !existing.is_placeholder {
                    tracing::warn!(
                        collection_name = %name,
                        held = %existing.collection_id,
                        offered = %schema.collection_id,
                        "Refusing to displace a cached collection with a different collection ID"
                    );
                    displaced = false;
                    return old.clone();
                }
            }
            let mut cache = old.clone();
            cache.insert(name.clone(), Collection::new(schema.clone()));
            cache
        });
        Ok(displaced)
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
        Ok(self.collections.peek(|cache| cache.contains_key(name)))
    }

    /// Find a collection by its collection ID (schema version ID).
    ///
    /// This is useful for P2P sync where we receive blocks with schema_version_id
    /// and need to find the corresponding collection.
    ///
    /// Uses the process-wide cache.
    pub fn find_collection_by_id(&self, collection_id: &str) -> Result<Option<Collection>> {
        Ok(self.collections.peek(|cache| {
            cache
                .values()
                .find(|c| c.collection_id() == collection_id)
                .cloned()
        }))
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
        Ok(CollectionSnapshot::new(self.collections.load_clone()))
    }
}
