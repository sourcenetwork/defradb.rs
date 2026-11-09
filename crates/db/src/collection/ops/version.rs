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

        txn.datastore()?
            .project_document(&name, "")
            .await
            .map_err(Error::Storage)?;

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
