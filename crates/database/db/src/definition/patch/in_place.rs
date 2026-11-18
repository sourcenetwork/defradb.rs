use super::*;

impl<S: Store> crate::database::DB<S> {
    /// Write a patch that keeps its collection's version: an `IsActive`-only
    /// change, a lens link (`PreviousVersion` only), or a
    /// metadata-only change. Returns `false` when the patch needs a new
    /// version instead.
    ///
    /// Its reads and writes run in `txn`, which the caller commits.
    pub(crate) async fn update_version_in_place(
        &self,
        txn: &mut DbTxn<S>,
        patched: &PendingPatch,
    ) -> Result<bool> {
        let PendingPatch {
            old_schema,
            new_schema,
            is_active_explicitly_set,
            ..
        } = patched;
        let is_active_explicitly_set = *is_active_explicitly_set;
        let mut new_schema = new_schema.clone();
        let old_version_id = &old_schema.version_id;
        let actual_name = &old_schema.name;
        let same_shape =
            new_schema.fields == old_schema.fields && new_schema.name == old_schema.name;

        let is_isactive_only_change = is_active_explicitly_set && same_shape;
        let is_transform_only_change = !is_active_explicitly_set
            && same_shape
            && new_schema.is_active == old_schema.is_active
            && new_schema.previous_version != old_schema.previous_version;
        let is_metadata_only_change = !is_active_explicitly_set
            && !is_transform_only_change
            && same_shape
            && new_schema.is_active == old_schema.is_active
            && new_schema.query == old_schema.query
            && new_schema.previous_version == old_schema.previous_version;

        if !(is_isactive_only_change || is_transform_only_change || is_metadata_only_change) {
            return Ok(false);
        }

        new_schema.version_id = old_version_id.clone();

        if !new_schema.is_active
            && old_schema.is_active
            && self.collection_has_data_with_txn(txn, old_schema).await?
        {
            return Err(Error::InvalidPatch(
                "cannot delete a collection that has documents, first delete the documents and then delete the version".to_string(),
            ));
        }

        let name_key = CollectionNameKey::new(actual_name);
        {
            let systemstore = txn.systemstore()?;
            let key = CollectionKey::new(old_version_id);
            let data = serde_json::to_vec(&new_schema).map_err(|e| {
                Error::collection_schema_json(
                    format!("failed to serialize updated schema version '{old_version_id}'"),
                    e,
                )
            })?;
            systemstore
                .set(&key.bytes(), &data)
                .await
                .map_err(Error::Storage)?;
            // The collection's name belongs to its active version: only this
            // version's own activation or deactivation moves it. An inactive
            // version that stays inactive leaves it with the active one.
            if new_schema.is_active {
                systemstore
                    .set(&name_key.bytes(), old_version_id.as_bytes())
                    .await
                    .map_err(Error::Storage)?;
            } else if old_schema.is_active {
                systemstore
                    .delete(&name_key.bytes())
                    .await
                    .map_err(Error::Storage)?;
            }
        }
        if new_schema.is_active || old_schema.is_active {
            let cached_collection = match new_schema.is_active {
                true => Some(
                    Collection::load_index_actions(new_schema.clone(), &txn.systemstore()?).await?,
                ),
                false => None,
            };
            let collections = std::sync::Arc::clone(&self.collections);
            let registered_name = actual_name.clone();
            txn.on_commit(OnCommit::OrderedAndWait(Box::new(move || {
                collections.rcu(|old| {
                    let mut cache = old.clone();
                    match &cached_collection {
                        Some(collection) => cache.put_named(&registered_name, collection.clone()),
                        None => {
                            cache.remove(&registered_name);
                        }
                    }
                    cache
                });
            })))?;
        }

        tracing::info!(
            collection = %actual_name,
            version = %old_version_id,
            is_active = new_schema.is_active,
            "Updated collection version in place"
        );
        Ok(true)
    }
}
