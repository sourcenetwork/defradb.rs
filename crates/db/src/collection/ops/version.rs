use super::*;
use storage::corekv::OnCommit;

impl<S: Store> crate::database::DB<S> {
    /// [`Self::set_active_collection_version_as`] in [`defra_core::RequestContext::ambient`].
    pub async fn set_active_collection_version(&self, version_id: &str) -> Result<()> {
        self.set_active_collection_version_as(&defra_core::RequestContext::ambient(), version_id)
            .await
    }

    /// Set the active collection version.
    ///
    /// This activates the collection with the given version ID and deactivates
    /// any other versions of the same collection.
    ///
    /// # Arguments
    ///
    /// * `version_id` - The version ID of the collection to activate
    ///
    /// # Errors
    ///
    /// - `CollectionVersionNotFound` if no collection with the given version ID exists
    #[instrument(skip(self), fields(version_id = %version_id), name = "db.set_active_version")]
    pub async fn set_active_collection_version_as(
        &self,
        context: &defra_core::RequestContext,
        version_id: &str,
    ) -> Result<()> {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await?;
        if version_id.is_empty() {
            return Err(Error::CollectionVersionIDEmpty);
        }

        // Load the target collection from persistent store by version_id
        let txn = self.new_txn(false).await?;

        // Extract the target schema and perform all systemstore operations in a block
        // so the systemstore reference is dropped before calling txn.commit()
        let (target_schema, name) = {
            let systemstore = txn.systemstore()?;

            let collection_key = CollectionKey::new(version_id);
            let target_bytes = systemstore
                .get(&collection_key.bytes())
                .await
                .map_err(Error::Storage)?
                .ok_or(Error::CollectionVersionNotFound(version_id.to_string()))?;

            let mut target_schema: CollectionVersion = serde_json::from_slice(&target_bytes)
                .map_err(|e| {
                    Error::collection_schema_json(
                        format!(
                            "failed to deserialize schema for version_id '{}'",
                            version_id
                        ),
                        e,
                    )
                })?;
            crate::collection::populate_collection_root_id(&systemstore, &mut target_schema)
                .await?;
            // A synced version binds its policy by CID. Rebuilt without the
            // reference, the record commits to less than its identity does,
            // and activating it would serve the collection unpoliced,
            // indistinguishable from one that never had a policy.
            if let (None, Some(policy_cid)) = (&target_schema.policy, &target_schema.policy_cid) {
                return Err(Error::CollectionVersionPolicyNotHeld {
                    version_id: version_id.to_string(),
                    policy_cid: policy_cid.clone(),
                });
            }

            let name = target_schema.name.clone();
            let collection_id = target_schema.collection_id.clone();

            // Update target to be active
            target_schema.is_active = true;

            // Store the updated target schema
            let target_data = serde_json::to_vec(&target_schema).map_err(|e| {
                Error::collection_schema_json(
                    format!("failed to serialize schema for version_id '{}'", version_id),
                    e,
                )
            })?;
            systemstore
                .set(&collection_key.bytes(), &target_data)
                .await
                .map_err(Error::Storage)?;

            // Update the name pointer to point to this version
            let name_key = CollectionNameKey::new(&name);
            systemstore
                .set(&name_key.bytes(), version_id.as_bytes())
                .await
                .map_err(Error::Storage)?;
            crate::definition::migration::owe_reindex(
                &systemstore,
                &self.collections,
                &collection_id,
                None,
            )
            .await?;

            // Find and deactivate other versions with the same collection_id
            let version_prefix = CollectionVersionKey::collection_prefix(&collection_id);
            let opts = IterOptions::new().with_prefix(version_prefix);
            let mut iter = systemstore.iterator(opts).await.map_err(Error::Storage)?;

            while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
                // Extract version_id from key: /collection/version/{collection_id}/{version_id}
                let key_str = String::from_utf8_lossy(&pair.key);
                if let Some(other_version_id) = key_str.rsplit('/').next() {
                    if other_version_id != version_id {
                        // Load, deactivate, and store the other version
                        let other_key = CollectionKey::new(other_version_id);
                        if let Some(other_bytes) = systemstore
                            .get(&other_key.bytes())
                            .await
                            .map_err(Error::Storage)?
                        {
                            match serde_json::from_slice::<CollectionVersion>(&other_bytes) {
                                Ok(mut other_schema) if other_schema.is_active => {
                                    other_schema.is_active = false;
                                    let other_data =
                                        serde_json::to_vec(&other_schema).map_err(|e| {
                                            Error::collection_schema_json(
                                                format!(
                                                    "failed to serialize schema for version_id '{}'",
                                                    other_version_id
                                                ),
                                                e,
                                            )
                                        })?;
                                    systemstore
                                        .set(&other_key.bytes(), &other_data)
                                        .await
                                        .map_err(Error::Storage)?;
                                }
                                Ok(_) => {}
                                // The name now points at the version being activated,
                                // and version scans skip an undecodable record
                                // (`get_all_collection_versions_with_txn`), so it is
                                // not served; it is reported rather than failing the switch.
                                Err(error) => tracing::warn!(
                                    version_id = %other_version_id,
                                    %error,
                                    "A stored collection version cannot be decoded, so it was not deactivated"
                                ),
                            }
                        }
                    }
                }
            }
            iter.close().await.map_err(Error::Storage)?;

            (target_schema, name)
        };

        let collection =
            Collection::load_index_actions(target_schema.clone(), &txn.systemstore()?).await?;
        let cache = std::sync::Arc::clone(&self.collections);
        let registered_name = name.clone();
        txn.on_commit(OnCommit::OrderedAndWait(Box::new(move || {
            cache.rcu(|old| {
                let mut cache = old.clone();
                cache.put_named(registered_name.as_str(), collection.clone());
                cache
            });
        })))?;
        txn.commit().await?;

        tracing::info!(
            collection_name = %name,
            version_id = %version_id,
            "Set active collection version"
        );

        // After switching the active version, rebuild the indexes the switch owes.
        self.settle_owed_reindexes_after_commit().await;

        Ok(())
    }

    /// Get a collection by version ID from the cache.
    ///
    /// This searches the in-memory cache for a collection with the given version ID.
    /// It only returns active collections that are in the cache.
    pub fn get_collection_by_version_id(&self, version_id: &str) -> Result<Option<Collection>> {
        Ok(self
            .collections
            .peek(|cache| cache.by_version(version_id).cloned()))
    }

    /// Get a collection by version ID, searching both cache and KV store.
    pub async fn get_collection_by_version_id_full(
        &self,
        version_id: &str,
    ) -> Result<Option<Collection>> {
        // Check cache first
        if let Some(c) = self.get_collection_by_version_id(version_id)? {
            return Ok(Some(c));
        }
        // Search all stored versions (including inactive)
        let all_versions = self.get_all_collection_versions().await?;
        Ok(all_versions
            .into_iter()
            .find(|v| v.version_id == version_id)
            .map(|schema| Collection::new(schema, self.crdts().clone())))
    }

    /// Get the active collection versions, from the in-memory cache.
    ///
    /// The cheap counterpart to `get_all_collection_versions`, which scans the
    /// whole `/collection/` prefix. A selector that does not ask for inactive
    /// versions is answerable from the cache alone, which is the distinction
    /// `CollectionSelector::needs_all_versions` draws.
    pub fn get_active_collection_versions(&self) -> Result<Vec<CollectionVersion>> {
        self.get_all_active_collections_internal()
    }

    /// Get all collection versions from storage (active and inactive).
    ///
    /// This scans `/collection/id/` prefix to load ALL versions, matching
    /// Go's behavior of loading all versions for cross-collection validation.
    pub async fn get_all_collection_versions(&self) -> Result<Vec<CollectionVersion>> {
        let mut txn = self.new_txn(true).await?;
        let versions = self.get_all_collection_versions_with_txn(&mut txn).await?;
        let _ = txn.discard();
        Ok(versions)
    }

    /// Get all collection versions visible in a transaction snapshot.
    pub(crate) async fn get_all_collection_versions_with_txn(
        &self,
        txn: &mut DbTxn<S>,
    ) -> Result<Vec<CollectionVersion>> {
        let mut versions = Vec::new();
        let prefix = CollectionKey::collection_prefix();

        {
            let systemstore = txn.systemstore()?;
            let opts = IterOptions::new().with_prefix(prefix).with_validation(true);
            let mut iter = systemstore.iterator(opts).await.map_err(Error::Storage)?;

            while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
                match serde_json::from_slice::<CollectionVersion>(&pair.value) {
                    Ok(mut col) => {
                        crate::collection::populate_collection_root_id(&systemstore, &mut col)
                            .await?;
                        versions.push(col);
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "Failed to deserialize collection version during scan"
                        );
                    }
                }
            }

            iter.close().await.map_err(Error::Storage)?;
        }

        Ok(versions)
    }

    /// [`Self::delete_collection_version_as`] in [`defra_core::RequestContext::ambient`].
    pub async fn delete_collection_version(
        &self,
        version_id: &str,
        also_deleting: &[String],
    ) -> Result<()> {
        self.delete_collection_version_as(
            &defra_core::RequestContext::ambient(),
            version_id,
            also_deleting,
        )
        .await
    }

    /// Delete a specific collection version by version_id.
    ///
    /// This performs true deletion (not deactivation): removes the version from
    /// the KV store, version index, name pointer (if active), blockstore blocks,
    /// and in-memory cache.
    ///
    /// # Validation
    ///
    /// - The version must exist
    /// - No non-deleted child versions may reference this as PreviousVersion
    ///   (unless those children are also in `also_deleting` batch)
    /// - The collection must have no documents
    pub async fn delete_collection_version_as(
        &self,
        context: &defra_core::RequestContext,
        version_id: &str,
        also_deleting: &[String],
    ) -> Result<()> {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await?;
        // Load the version from KV store
        let txn = self.new_txn(true).await?;
        let target_schema = {
            let systemstore = txn.systemstore()?;
            let collection_key = CollectionKey::new(version_id);
            let target_bytes = systemstore
                .get(&collection_key.bytes())
                .await
                .map_err(Error::Storage)?
                .ok_or(Error::CollectionVersionNotFound(version_id.to_string()))?;
            let mut schema =
                serde_json::from_slice::<CollectionVersion>(&target_bytes).map_err(|e| {
                    Error::collection_schema_json(
                        format!("failed to deserialize version '{}'", version_id),
                        e,
                    )
                })?;

            crate::collection::populate_collection_root_id(&systemstore, &mut schema).await?;

            schema
        };
        let _ = txn.discard();

        let collection_id = target_schema.collection_id.clone();
        let name = target_schema.name.clone();

        // Validate: no child versions reference this (excluding batch siblings)
        let all_versions = self.get_all_collection_versions().await?;
        for other in &all_versions {
            if let Some(ref prev) = other.previous_version {
                if prev.source_collection_id == version_id
                    && !also_deleting.contains(&other.version_id)
                {
                    return Err(Error::InvalidPatch(
                        "cannot delete a version that is used by a newer version, first delete the new version".to_string(),
                    ));
                }
            }
        }
        let mut deleting_versions: rapidhash::RapidHashSet<&str> =
            also_deleting.iter().map(String::as_str).collect();
        deleting_versions.insert(version_id);
        let is_deleting_last_local_version = all_versions.iter().all(|version| {
            version.collection_id != collection_id
                || deleting_versions.contains(version.version_id.as_str())
        });

        // Validate: no documents exist
        if target_schema.is_active {
            let has_data = self.collection_has_data(&target_schema).await?;
            if has_data {
                return Err(Error::InvalidPatch(
                    "cannot delete a collection that has documents, first delete the documents and then delete the version".to_string(),
                ));
            }
        }

        // Perform the actual deletion
        let txn = self.new_txn(false).await?;
        {
            let systemstore = txn.systemstore()?;

            // 1. Delete /collection/id/{version_id}
            let collection_key = CollectionKey::new(version_id);
            systemstore
                .delete(&collection_key.bytes())
                .await
                .map_err(Error::Storage)?;

            // 2. Delete /collection/version/{collection_id}/{version_id}
            let version_index_key = CollectionVersionKey::new(&collection_id, version_id);
            systemstore
                .delete(&version_index_key.bytes())
                .await
                .map_err(Error::Storage)?;

            // 3. If version was active, delete /collection/name/{name}
            if target_schema.is_active {
                let name_key = CollectionNameKey::new(&name);
                systemstore
                    .delete(&name_key.bytes())
                    .await
                    .map_err(Error::Storage)?;
            }
        }

        // 4. Delete IPLD blocks from blockstore (collection CID + field CIDs)
        {
            let blockstore = txn.blockstore()?;
            // Delete the version CID block itself
            if let Ok(cid) = cid::Cid::try_from(version_id) {
                let _ = blockstore.delete(&cid.to_bytes()).await;
            }
            // Delete field CID blocks
            for field in &target_schema.fields {
                if !field.id.is_empty() {
                    if let Ok(cid) = cid::Cid::try_from(field.id.as_str()) {
                        let _ = blockstore.delete(&cid.to_bytes()).await;
                    }
                }
            }
        }

        if target_schema.is_active {
            let cache = std::sync::Arc::clone(&self.collections);
            let name_for_cache = name.clone();
            txn.on_commit(OnCommit::OrderedAndWait(Box::new(move || {
                cache.rcu(|old| {
                    let mut cache = old.clone();
                    cache.remove(&name_for_cache);
                    cache
                });
            })))?;
        }
        txn.commit().await?;

        // 5. Remove from in-memory cache if active (ordered callback above).
        if is_deleting_last_local_version {
            self.forbid_collection_id(&collection_id)?;
            self.vector_codes
                .evict_collection(target_schema.resolved_root_id());
        }

        tracing::info!(
            version_id = %version_id,
            collection_name = %name,
            "Deleted collection version"
        );

        Ok(())
    }

    /// [`Self::delete_collection_versions_batch_as`] in [`defra_core::RequestContext::ambient`].
    pub async fn delete_collection_versions_batch(&self, version_ids: Vec<String>) -> Result<()> {
        self.delete_collection_versions_batch_as(
            &defra_core::RequestContext::ambient(),
            version_ids,
        )
        .await
    }

    /// Delete multiple collection versions in a single batch.
    ///
    /// Versions are sorted topologically (children before parents based on
    /// PreviousVersion) and deleted in that order. When checking for child
    /// references, other versions in the batch are excluded from validation.
    pub async fn delete_collection_versions_batch_as(
        &self,
        context: &defra_core::RequestContext,
        version_ids: Vec<String>,
    ) -> Result<()> {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await?;
        if version_ids.is_empty() {
            return Ok(());
        }

        // Sort topologically: children before parents
        // A child has a PreviousVersion pointing to a parent in the batch
        let all_versions = self.get_all_collection_versions().await?;
        let version_map: rapidhash::RapidHashMap<&str, &CollectionVersion> = all_versions
            .iter()
            .map(|v| (v.version_id.as_str(), v))
            .collect();
        let deleting_versions: rapidhash::RapidHashSet<&str> =
            version_ids.iter().map(String::as_str).collect();

        for version_id in &version_ids {
            if !version_map.contains_key(version_id.as_str()) {
                return Err(Error::CollectionVersionNotFound(version_id.clone()));
            }
        }

        for version in &all_versions {
            if deleting_versions.contains(version.version_id.as_str()) {
                if version.is_active && self.collection_has_data(version).await? {
                    return Err(Error::InvalidPatch(
                        "cannot delete a collection that has documents, first delete the documents and then delete the version".to_string(),
                    ));
                }
                continue;
            }

            if version.previous_version.as_ref().is_some_and(|previous| {
                deleting_versions.contains(previous.source_collection_id.as_str())
            }) {
                return Err(Error::InvalidPatch(
                    "cannot delete a version that is used by a newer version, first delete the new version".to_string(),
                ));
            }
        }

        let removed_collection_ids: rapidhash::RapidHashSet<&str> = all_versions
            .iter()
            .filter(|version| {
                deleting_versions.contains(version.version_id.as_str())
                    && all_versions.iter().all(|other| {
                        other.collection_id != version.collection_id
                            || deleting_versions.contains(other.version_id.as_str())
                    })
            })
            .map(|version| version.collection_id.as_str())
            .collect();

        if all_versions.iter().any(|version| {
            !deleting_versions.contains(version.version_id.as_str())
                && version.fields.iter().any(|field| {
                    field
                        .kind
                        .relation_collection_id()
                        .is_some_and(|id| removed_collection_ids.contains(id))
                })
        }) {
            return Err(Error::InvalidPatch(
                "cannot remove a collection while another field references it".to_string(),
            ));
        }

        let mut sorted = Vec::with_capacity(version_ids.len());
        let mut remaining: rapidhash::RapidHashSet<String> = version_ids.iter().cloned().collect();

        // Simple topological sort: repeatedly find versions whose children
        // (within the batch) have already been added to sorted
        let max_iterations = version_ids.len() * version_ids.len();
        let mut iterations = 0;
        while !remaining.is_empty() {
            iterations += 1;
            if iterations > max_iterations {
                // Break cycles by just adding remaining in any order
                for id in &remaining {
                    sorted.push(id.clone());
                }
                break;
            }

            let mut added_any = false;
            let remaining_snapshot: Vec<String> = remaining.iter().cloned().collect();
            for id in &remaining_snapshot {
                // Check if any remaining version has this as its parent
                let has_remaining_child = remaining.iter().any(|other_id| {
                    if other_id == id {
                        return false;
                    }
                    version_map
                        .get(other_id.as_str())
                        .and_then(|v| v.previous_version.as_ref())
                        .map(|prev| prev.source_collection_id == *id)
                        .unwrap_or(false)
                });

                if !has_remaining_child {
                    sorted.push(id.clone());
                    remaining.remove(id);
                    added_any = true;
                }
            }

            if !added_any {
                // All remaining have circular deps, add them all
                for id in &remaining {
                    sorted.push(id.clone());
                }
                break;
            }
        }

        // Delete each version in topological order
        for version_id in &sorted {
            self.delete_collection_version_as(context, version_id, &version_ids)
                .await?;
        }

        Ok(())
    }

    /// Check if a collection has any documents in the datastore.
    pub(crate) async fn collection_has_data(
        &self,
        version: &schema::CollectionVersion,
    ) -> Result<bool> {
        if !version.is_materialized {
            return Ok(false);
        }

        let txn = self.new_txn(true).await?;
        let has_data = {
            let datastore = txn.datastore()?;

            let prefix = if version.query.is_some() {
                // View: check view cache prefix using short ID
                storage::keys::datastore::ViewCacheKey::collection_prefix(version.root_id)
            } else {
                // Regular collection: check document prefix
                format!("/d/{}/", version.collection_id).as_bytes().to_vec()
            };

            let opts = IterOptions::new().with_prefix(prefix);
            let mut iter = datastore.iterator(opts).await.map_err(Error::Storage)?;
            let has_any = iter.next().await.map_err(Error::Storage)?.is_some();
            iter.close().await.map_err(Error::Storage)?;
            has_any
        };
        let _ = txn.discard();
        Ok(has_data)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn validated_collection_version_scan_detects_phantom_create() {
        workers::block_on(async {
            let db = crate::DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap();
            let mut initial =
                query::parse_sdl("type ExistingSchemaProbe { value: String }").unwrap();
            db.create_collection(initial.remove(0)).await.unwrap();

            let mut schema_txn = db.new_txn(false).await.unwrap();
            db.get_all_collection_versions_with_txn(&mut schema_txn)
                .await
                .unwrap();
            let mut staged = query::parse_sdl("type StagedSchemaProbe { value: String }").unwrap();
            db.create_collection_with_txn(&mut schema_txn, staged.remove(0))
                .await
                .unwrap();

            let mut concurrent =
                query::parse_sdl("type ConcurrentSchemaProbe { value: String }").unwrap();
            db.create_collection(concurrent.remove(0)).await.unwrap();

            let result = schema_txn.commit().await;
            assert!(
                result.as_ref().is_err_and(|error| error.is_txn_conflict()),
                "expected stale collection scan to conflict, got {result:?}"
            );
        })
        .unwrap()
    }
}
