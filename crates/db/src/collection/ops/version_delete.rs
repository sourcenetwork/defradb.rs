use super::*;
use storage::corekv::OnCommit;

impl<S: Store> crate::database::DB<S> {
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
        let mut txn = self.new_txn(false).await?;
        let all_versions = self.get_all_collection_versions_with_txn(&mut txn).await?;
        let Some(target) = all_versions.iter().find(|v| v.version_id == version_id) else {
            return Err(Error::CollectionVersionNotFound(version_id.to_string()));
        };
        for other in &all_versions {
            if other
                .previous_version
                .as_ref()
                .is_some_and(|prev| prev.source_collection_id == version_id)
                && !also_deleting.contains(&other.version_id)
            {
                return Err(version_in_use());
            }
        }
        if target.is_active && self.collection_has_data(&txn, target).await? {
            return Err(collection_has_documents());
        }

        let mut deleting: rapidhash::RapidHashSet<&str> =
            also_deleting.iter().map(String::as_str).collect();
        deleting.insert(version_id);
        self.write_version_deletion(&txn, target).await?;
        txn.commit().await?;
        self.forget_removed_collections(&all_versions, &deleting, std::slice::from_ref(target))
    }

    /// [`Self::delete_collection_versions_batch_as`] in [`defra_core::RequestContext::ambient`].
    pub async fn delete_collection_versions_batch(&self, version_ids: Vec<String>) -> Result<()> {
        self.delete_collection_versions_batch_as(
            &defra_core::RequestContext::ambient(),
            version_ids,
        )
        .await
    }

    /// Delete several collection versions in one transaction: all of them or
    /// none. Go's patch removal of `/<VersionID>` or `/<Name>` entries.
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
        let mut txn = self.new_txn(false).await?;
        let all_versions = self.get_all_collection_versions_with_txn(&mut txn).await?;
        let deleting: rapidhash::RapidHashSet<&str> =
            version_ids.iter().map(String::as_str).collect();
        let targets = self
            .check_versions_removable(&txn, &all_versions, &deleting)
            .await?;
        for target in &targets {
            self.write_version_deletion(&txn, target).await?;
        }
        txn.commit().await?;
        self.forget_removed_collections(&all_versions, &deleting, &targets)
    }

    /// Go's checks on a patch that removes `deleting` from `all_versions`, as
    /// `txn` sees them: every version exists, none has documents, no surviving
    /// version builds on one, and no surviving field references a removed
    /// collection. The versions to delete.
    pub(crate) async fn check_versions_removable(
        &self,
        txn: &DbTxn<S>,
        all_versions: &[CollectionVersion],
        deleting: &rapidhash::RapidHashSet<&str>,
    ) -> Result<Vec<CollectionVersion>> {
        let targets: Vec<CollectionVersion> = all_versions
            .iter()
            .filter(|v| deleting.contains(v.version_id.as_str()))
            .cloned()
            .collect();
        if let Some(missing) = deleting
            .iter()
            .find(|id| !targets.iter().any(|t| t.version_id == **id))
        {
            return Err(Error::CollectionVersionNotFound(missing.to_string()));
        }
        for target in &targets {
            if target.is_active && self.collection_has_data(txn, target).await? {
                return Err(collection_has_documents());
            }
        }
        let surviving: Vec<CollectionVersion> = all_versions
            .iter()
            .filter(|v| !deleting.contains(v.version_id.as_str()))
            .cloned()
            .collect();
        if surviving.iter().any(|version| {
            version
                .previous_version
                .as_ref()
                .is_some_and(|prev| deleting.contains(prev.source_collection_id.as_str()))
        }) {
            return Err(version_in_use());
        }
        schema::definition_validation::validate_collection_changes(all_versions, &surviving)
            .map_err(Error::InvalidPatch)?;
        Ok(targets)
    }

    /// Stage the deletion of `target` on `txn`: its version keys, its name
    /// pointer when active, its blocks, and its cache entry once `txn` commits.
    pub(crate) async fn write_version_deletion(
        &self,
        txn: &DbTxn<S>,
        target: &CollectionVersion,
    ) -> Result<()> {
        let version_id = target.version_id.as_str();
        {
            let systemstore = txn.systemstore()?;
            systemstore
                .delete(&CollectionKey::new(version_id).bytes())
                .await
                .map_err(Error::Storage)?;
            systemstore
                .delete(&CollectionVersionKey::new(&target.collection_id, version_id).bytes())
                .await
                .map_err(Error::Storage)?;
            if target.is_active {
                systemstore
                    .delete(&CollectionNameKey::new(&target.name).bytes())
                    .await
                    .map_err(Error::Storage)?;
            }
        }
        if target.is_active {
            txn.datastore()?
                .project_document(&target.name, "")
                .await
                .map_err(Error::Storage)?;
        }
        {
            let blockstore = txn.blockstore()?;
            let field_cids = target.fields.iter().map(|field| field.id.as_str());
            for cid in std::iter::once(version_id).chain(field_cids) {
                if let Ok(cid) = cid::Cid::try_from(cid) {
                    let _ = blockstore.delete(&cid.to_bytes()).await;
                }
            }
        }
        if target.is_active {
            let cache = std::sync::Arc::clone(&self.collections);
            let name = target.name.clone();
            txn.on_commit(OnCommit::OrderedAndWait(Box::new(move || {
                cache.rcu(|old| {
                    let mut cache = old.clone();
                    cache.remove(&name);
                    cache
                });
            })))?;
        }
        tracing::info!(version_id, collection_name = %target.name, "Deleted collection version");
        Ok(())
    }

    /// After a committed deletion of `deleting`, forbid the IDs of the
    /// collections none of whose versions remain.
    pub(crate) fn forget_removed_collections(
        &self,
        all_versions: &[CollectionVersion],
        deleting: &rapidhash::RapidHashSet<&str>,
        targets: &[CollectionVersion],
    ) -> Result<()> {
        for target in targets {
            let is_last_local_version = all_versions.iter().all(|version| {
                version.collection_id != target.collection_id
                    || deleting.contains(version.version_id.as_str())
            });
            if is_last_local_version {
                self.forbid_collection_id(&target.collection_id)?;
                self.vector_codes
                    .evict_collection(target.resolved_root_id());
            }
        }
        Ok(())
    }
}

fn version_in_use() -> Error {
    Error::InvalidPatch(
        "cannot delete a version that is used by a newer version, first delete the new version"
            .to_string(),
    )
}

fn collection_has_documents() -> Error {
    Error::InvalidPatch(
        "cannot delete a collection that has documents, first delete the documents and then delete the version".to_string(),
    )
}
