use super::*;

impl<S: Store> crate::database::DB<S> {
    /// Stage `prepared` over the version it started from, on `txn`: Go's
    /// in-place update for an `IsActive` toggle, a migration link, or a
    /// metadata change. `siblings` are the other collections the same patch
    /// changes and has not staged yet.
    pub(crate) async fn write_in_place(
        &self,
        txn: &mut DbTxn<S>,
        prepared: &PreparedPatch,
        siblings: &[CollectionVersion],
    ) -> Result<PatchOutcome> {
        let old_schema = &prepared.old_schema;
        let old_version_id = old_schema.version_id.clone();
        let actual_name = old_schema.name.clone();
        let collection_id = old_schema.collection_id.clone();
        let mut new_schema = prepared.new_schema.clone();

        // Keep original version_id
        new_schema.version_id = old_version_id.clone();
        // For IsActive-only, metadata-only, restore original previous_version.
        // For Transform-only changes, keep the new previous_version (contains the transform).
        if !prepared.is_transform_only() {
            new_schema.previous_version = old_schema.previous_version.clone();
        }

        // Validate: can't remove a collection that has documents (only on active→inactive)
        if !new_schema.is_active && old_schema.is_active {
            let has_data = self.collection_has_data(old_schema).await?;
            if has_data {
                return Err(Error::InvalidPatch(
                    "cannot delete a collection that has documents, first delete the documents and then delete the version".to_string(),
                ));
            }
        }

        // Run cross-collection validators to catch issues like multiple active versions
        // against the same snapshot that will store the in-place update.
        let all_existing = self.get_all_collection_versions_with_txn(txn).await?;
        let Some(persisted_old_schema) = all_existing.iter().find(|version| {
            version.collection_id == collection_id && version.version_id == old_version_id
        }) else {
            return Err(Error::CollectionVersionNotFound(old_version_id.clone()));
        };
        if !crate::collection::loader::same_schema(persisted_old_schema, old_schema) {
            return Err(Error::Storage(storage::Error::TxnConflict));
        }
        let name_key = CollectionNameKey::new(&actual_name);
        let persisted_name_version = txn
            .systemstore()?
            .get(&name_key.bytes())
            .await
            .map_err(Error::Storage)?;
        if old_schema.is_active
            && persisted_name_version.as_deref() != Some(old_version_id.as_bytes())
        {
            return Err(Error::Storage(storage::Error::TxnConflict));
        }
        let new_collections: Vec<CollectionVersion> = with_siblings(&all_existing, siblings)
            .into_iter()
            .filter(|c| c.version_id != old_version_id)
            .chain(std::iter::once(new_schema.clone()))
            .collect();
        schema::definition_validation::validate_collection_changes(&all_existing, &new_collections)
            .map_err(Error::InvalidPatch)?;

        // Store the updated version
        {
            let systemstore = txn.systemstore()?;
            let key = CollectionKey::new(&old_version_id);
            let data = serde_json::to_vec(&new_schema).map_err(|e| {
                Error::collection_schema_json(
                    format!(
                        "failed to serialize updated schema version '{}'",
                        old_version_id
                    ),
                    e,
                )
            })?;
            systemstore
                .set(&key.bytes(), &data)
                .await
                .map_err(Error::Storage)?;

            // Update name pointer based on activation state
            if new_schema.is_active {
                systemstore
                    .set(&name_key.bytes(), old_version_id.as_bytes())
                    .await
                    .map_err(Error::Storage)?;
            } else {
                systemstore
                    .delete(&name_key.bytes())
                    .await
                    .map_err(Error::Storage)?;
            }
        }
        let cached_collection =
            Collection::load_index_actions(new_schema.clone(), &txn.systemstore()?).await?;
        let collections = std::sync::Arc::clone(&self.collections);
        let registered_name = actual_name.clone();
        let active = new_schema.is_active;
        txn.on_commit(OnCommit::OrderedAndWait(Box::new(move || {
            collections.rcu(|old| {
                let mut cache = old.clone();
                if active {
                    cache.put_named(&registered_name, cached_collection.clone());
                } else {
                    cache.remove(&registered_name);
                }
                cache
            });
        })))?;

        tracing::info!(
            collection = %actual_name,
            version = %old_version_id,
            is_active = new_schema.is_active,
            "Updated collection version in place"
        );

        Ok(PatchOutcome {
            schema: new_schema,
            committed_migration: None,
            is_new_version: false,
        })
    }
}
