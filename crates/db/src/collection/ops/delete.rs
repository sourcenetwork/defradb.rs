use super::*;
use rapidhash::HashSetExt;
use storage::corekv::OnCommit;

impl<S: Store> crate::database::DB<S> {
    /// Delete a collection within an existing transaction.
    ///
    /// This removes:
    /// 1. The collection schema from `/collection/id/{version_id}`
    /// 2. The name mapping from `/collection/name/{name}`
    /// 3. The version index entry from `/collection/version/{collection_id}/{version_id}`
    ///
    /// Note: This does NOT delete documents. Use `truncate_collection` first if needed.
    ///
    /// Returns the collection's short ID.
    #[instrument(skip(self, txn), fields(collection = %name), name = "db.delete_collection")]
    pub(crate) async fn delete_collection_with_txn(
        &self,
        txn: &mut DbTxn<S>,
        name: &str,
    ) -> Result<u32> {
        // Get the collection to find its version_id and collection_id
        let collection = txn
            .get_collection(name)
            .await?
            .ok_or_else(|| Error::CollectionNotFound(name.to_string()))?;

        let version_id = collection.version_id().to_string();
        let collection_id = collection.collection_id().to_string();

        let systemstore = txn.systemstore()?;

        // 1. Delete the full schema from /collection/id/{version_id}
        let collection_key = CollectionKey::new(&version_id);
        systemstore
            .delete(&collection_key.bytes())
            .await
            .map_err(Error::Storage)?;

        // 2. Delete the name mapping from /collection/name/{name}
        let name_key = CollectionNameKey::new(name);
        systemstore
            .delete(&name_key.bytes())
            .await
            .map_err(Error::Storage)?;

        // 3. Delete the version index entry from /collection/version/{collection_id}/{version_id}
        let version_index_key = CollectionVersionKey::new(&collection_id, &version_id);
        systemstore
            .delete(&version_index_key.bytes())
            .await
            .map_err(Error::Storage)?;

        // Remove from transaction's cache
        txn.uncache_collection(name);

        tracing::info!(
            collection_name = %name,
            version_id = %version_id,
            collection_id = %collection_id,
            "Deleted collection"
        );

        Ok(collection.schema().resolved_root_id())
    }

    /// [`Self::delete_collection_as`] in [`defra_core::RequestContext::ambient`].
    pub async fn delete_collection(&self, name: &str) -> Result<()> {
        self.delete_collection_as(&defra_core::RequestContext::ambient(), name)
            .await
    }

    /// Delete a collection.
    ///
    /// This creates a new transaction, calls `delete_collection_with_txn`, commits,
    /// and updates the process-wide cache.
    ///
    /// Note: This does NOT delete documents. Use `truncate_collection` first if needed.
    #[instrument(skip(self), fields(collection = %name), name = "db.delete_collection_auto")]
    pub async fn delete_collection_as(
        &self,
        context: &defra_core::RequestContext,
        name: &str,
    ) -> Result<()> {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await?;
        let mut txn = self.new_txn(false).await?;

        match self.delete_collection_with_txn(&mut txn, name).await {
            Ok(short_id) => {
                let collections = std::sync::Arc::clone(&self.collections);
                let codes = std::sync::Arc::clone(&self.vector_codes);
                let name = name.to_owned();
                txn.on_commit(OnCommit::OrderedAndWait(Box::new(move || {
                    collections.rcu(|old| {
                        let mut cache = old.clone();
                        cache.remove(&name);
                        cache
                    });
                    codes.evict_collection(short_id);
                })))?;
                txn.commit().await?;

                Ok(())
            }
            Err(e) => {
                if let Err(discard_err) = txn.discard() {
                    tracing::warn!(
                        error = %discard_err,
                        original_error = %e,
                        "Transaction discard failed after delete_collection error"
                    );
                }
                Err(e)
            }
        }
    }

    /// [`Self::delete_collections_as`] in [`defra_core::RequestContext::ambient`].
    pub async fn delete_collections(&self, names: Vec<String>, active_only: bool) -> Result<()> {
        self.delete_collections_as(&defra_core::RequestContext::ambient(), names, active_only)
            .await
    }

    /// Delete one or more collections by name in a single call (Go #4688 parity).
    ///
    /// - If `active_only` is true, only the active head version of each named
    ///   collection is removed; earlier versions are kept intact.
    /// - If `active_only` is false (Go's default), every version of each named
    ///   collection is removed.
    ///
    /// Names are deduplicated. Name resolution happens up-front: if any name
    /// is unknown, no deletion is performed. Validation (no documents, no
    /// orphan child versions outside the batch) is delegated to
    /// `delete_collection_versions_batch`.
    pub async fn delete_collections_as(
        &self,
        context: &defra_core::RequestContext,
        names: Vec<String>,
        active_only: bool,
    ) -> Result<()> {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await?;
        if names.is_empty() {
            return Err(Error::InvalidPatch(
                "collection name required: pass at least one name to delete".into(),
            ));
        }
        if names.iter().any(String::is_empty) {
            return Err(Error::InvalidPatch("collection name can't be empty".into()));
        }

        let mut seen_names = rapidhash::RapidHashSet::new();
        let unique_names: Vec<String> = names
            .into_iter()
            .filter(|n| seen_names.insert(n.clone()))
            .collect();

        let mut version_ids: Vec<String> = Vec::new();
        let mut seen_versions = rapidhash::RapidHashSet::new();

        if active_only {
            for name in &unique_names {
                let col = self
                    .get_collection(name)?
                    .ok_or_else(|| Error::CollectionNotFound(name.clone()))?;
                let vid = col.version_id().to_string();
                if seen_versions.insert(vid.clone()) {
                    version_ids.push(vid);
                }
            }
        } else {
            let all_versions = self.get_all_collection_versions().await?;
            for name in &unique_names {
                let col = self
                    .get_collection(name)?
                    .ok_or_else(|| Error::CollectionNotFound(name.clone()))?;
                let collection_id = col.collection_id().to_string();
                for v in &all_versions {
                    if v.collection_id == collection_id
                        && seen_versions.insert(v.version_id.clone())
                    {
                        version_ids.push(v.version_id.clone());
                    }
                }
            }
        }

        self.delete_collection_versions_batch_as(context, version_ids)
            .await
    }
}
