use super::*;

/// Whether [`crate::database::DB::add_collection_to_cache`] took the schema.
///
/// The attribute sits on the type rather than on the method deliberately: a
/// method's `#[must_use]` is satisfied by the `map_err` every caller applies,
/// and the value falling out of `?` is then an ordinary expression statement
/// that nothing lints. A `#[must_use]` type is linted wherever it is dropped.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cached {
    /// The schema is now the cached entry for its name.
    Taken,
    /// Another collection holds that name, so the cache was left alone.
    NameHeldByAnother,
}

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
        let cache = self.collections.read().map_err(|e| {
            tracing::error!(error = ?e, "Collection cache lock poisoned during list");
            Error::LockPoisoned("collection cache lock poisoned during list".into())
        })?;
        Ok(cache.keys().cloned().collect())
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
    pub fn add_collection_to_cache(&self, schema: CollectionVersion) -> Result<Cached> {
        let name = schema.name.clone();
        let mut cache = self.collections.write().map_err(|e| {
            tracing::error!(error = ?e, collection_name = %name, "Collection cache lock poisoned during add_collection_to_cache");
            Error::LockPoisoned(
                "collection cache lock poisoned during add_collection_to_cache".into(),
            )
        })?;

        // The cache is keyed by name, but a collection's identity is its
        // collection ID. An entry naming a different collection must not be
        // replaced: whatever that collection knows and the incoming schema
        // does not carry would be dropped silently. A placeholder is a
        // stand-in for a definition that has not arrived, so it always yields.
        if let Some(existing) = cache.get(&name) {
            let existing = existing.schema();
            if existing.collection_id != schema.collection_id && !existing.is_placeholder {
                tracing::warn!(
                    collection_name = %name,
                    held = %existing.collection_id,
                    offered = %schema.collection_id,
                    "Refusing to displace a cached collection with a different collection ID"
                );
                return Ok(Cached::NameHeldByAnother);
            }
        }

        cache.insert(name, Collection::new(schema));
        Ok(Cached::Taken)
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
        let cache = self.collections.read().map_err(|e| {
            tracing::error!(error = ?e, collection_name = %name, "Collection cache lock poisoned during get");
            Error::LockPoisoned("collection cache lock poisoned during get".into())
        })?;
        Ok(cache.get(name).cloned())
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
        let cache = self.collections.read().map_err(|e| {
            tracing::error!(error = ?e, collection_name = %name, "Collection cache lock poisoned during has_collection");
            Error::LockPoisoned("collection cache lock poisoned during has_collection".into())
        })?;
        Ok(cache.contains_key(name))
    }

    /// Find a collection by its collection ID (schema version ID).
    ///
    /// This is useful for P2P sync where we receive blocks with schema_version_id
    /// and need to find the corresponding collection.
    ///
    /// Uses the process-wide cache.
    pub fn find_collection_by_id(&self, collection_id: &str) -> Result<Option<Collection>> {
        let cache = self.collections.read().map_err(|e| {
            tracing::error!(
                error = ?e,
                collection_id = %collection_id,
                "Collection cache lock poisoned during find_collection_by_id"
            );
            Error::LockPoisoned(
                "collection cache lock poisoned during find_collection_by_id".into(),
            )
        })?;
        Ok(cache
            .values()
            .find(|c| c.collection_id() == collection_id)
            .cloned())
    }

    pub(crate) fn forbid_collection_id(&self, collection_id: &str) -> Result<()> {
        let mut forbidden = self.forbidden_collection_ids.write().map_err(|e| {
            tracing::error!(
                error = ?e,
                collection_id = %collection_id,
                "Forbidden collection lock poisoned during forbid"
            );
            Error::LockPoisoned("forbidden collection lock poisoned during forbid".into())
        })?;
        forbidden.insert(collection_id.to_string());
        Ok(())
    }

    pub(crate) fn unforbid_collection_id(&self, collection_id: &str) -> Result<()> {
        let mut forbidden = self.forbidden_collection_ids.write().map_err(|e| {
            tracing::error!(
                error = ?e,
                collection_id = %collection_id,
                "Forbidden collection lock poisoned during unforbid"
            );
            Error::LockPoisoned("forbidden collection lock poisoned during unforbid".into())
        })?;
        forbidden.remove(collection_id);
        Ok(())
    }

    pub(crate) fn is_collection_forbidden(&self, collection_id: &str) -> Result<bool> {
        let forbidden = self.forbidden_collection_ids.read().map_err(|e| {
            tracing::error!(
                error = ?e,
                collection_id = %collection_id,
                "Forbidden collection lock poisoned during lookup"
            );
            Error::LockPoisoned("forbidden collection lock poisoned during lookup".into())
        })?;
        Ok(forbidden.contains(collection_id))
    }

    /// Get a snapshot of all collections (for use by DbTransactionRegistry).
    ///
    /// Returns an immutable snapshot that provides snapshot isolation for transactions.
    pub fn collections_snapshot(&self) -> Result<CollectionSnapshot> {
        let cache = self.collections.read().map_err(|e| {
            tracing::error!(error = ?e, "Collection cache lock poisoned during snapshot");
            Error::LockPoisoned("collection cache lock poisoned during snapshot".into())
        })?;
        Ok(CollectionSnapshot::new(cache.clone()))
    }
}
