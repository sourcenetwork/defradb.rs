//! Collection listing, activation and deletion inside a transaction.

use super::*;
use rapidhash::HashSetExt;
use storage::corekv::OnCommit;

impl<S: Store + 'static> DbTransactionRegistry<S> {
    /// [`Self::get_collections_in_txn_as`] in [`RequestContext::ambient`].
    pub async fn get_collections_in_txn(
        &self,
        txn_id: &str,
    ) -> Result<Vec<schema::CollectionVersion>> {
        self.get_collections_in_txn_as(&RequestContext::ambient(), txn_id)
            .await
    }

    /// Get all collection versions visible within a transaction.
    ///
    /// This reads from the transaction's systemstore, which includes both
    /// committed data and any uncommitted writes made within this transaction
    /// (e.g., placeholders from `set_migration_in_txn`).
    pub async fn get_collections_in_txn_as(
        &self,
        context: &RequestContext,
        txn_id: &str,
    ) -> Result<Vec<schema::CollectionVersion>> {
        let db = self.db.clone();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;

                let systemstore = txn.systemstore()?;
                let prefix = storage::keys::systemstore::CollectionKey::collection_prefix();
                let opts = storage::corekv::IterOptions::new().with_prefix(prefix);
                let mut iter = systemstore.iterator(opts).await.map_err(Error::Storage)?;

                let mut versions = Vec::new();
                while let Some(pair) = iter.next().await.map_err(Error::Storage)? {
                    match serde_json::from_slice::<schema::CollectionVersion>(&pair.value) {
                        Ok(mut col) => {
                            crate::collection::populate_collection_root_id(&systemstore, &mut col)
                                .await?;
                            if db.is_collection_forbidden(&col.collection_id)?
                                && !txn.was_collection_created(&col.collection_id)
                            {
                                continue;
                            }
                            versions.push(col);
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                "Failed to deserialize collection version during txn scan"
                            );
                        }
                    }
                }
                iter.close().await.map_err(Error::Storage)?;

                Ok(versions)
            })
        })
        .await
    }

    /// [`Self::set_collection_active_in_txn_as`] in [`RequestContext::ambient`].
    pub async fn set_collection_active_in_txn(
        &self,
        txn_id: &str,
        version_id: &str,
        is_active: bool,
    ) -> Result<schema::CollectionVersion> {
        self.set_collection_active_in_txn_as(
            &RequestContext::ambient(),
            txn_id,
            version_id,
            is_active,
        )
        .await
    }

    pub async fn set_collection_active_in_txn_as(
        &self,
        context: &RequestContext,
        txn_id: &str,
        version_id: &str,
        is_active: bool,
    ) -> Result<schema::CollectionVersion> {
        self.db
            .check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await?;
        let db = self.db.clone();
        let version_id = version_id.to_owned();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let version_id = version_id.as_str();
                let txn = owned.alone()?;
                let systemstore = txn.systemstore()?;
                let key = storage::keys::systemstore::CollectionKey::new(version_id);
                let data = systemstore
                    .get(&key.bytes())
                    .await
                    .map_err(Error::Storage)?
                    .ok_or_else(|| Error::CollectionVersionNotFound(version_id.to_string()))?;
                let mut target: schema::CollectionVersion =
                    serde_json::from_slice(&data).map_err(|error| {
                        Error::collection_schema_json(
                            format!("failed to deserialize schema version '{}'", version_id),
                            error,
                        )
                    })?;
                crate::collection::populate_collection_root_id(&systemstore, &mut target).await?;
                crate::chunk::ensure_admin_changeable(&target)?;
                let was_active = target.is_active;

                // The exact CollectionKey read above is a commit dependency. Document
                // writes read that same key, so a concurrent schema change conflicts.

                if was_active && !is_active && target.is_materialized {
                    let datastore = txn.datastore()?;
                    let prefix = if target.query.is_some() {
                        storage::keys::datastore::ViewCacheKey::collection_prefix(target.root_id)
                    } else {
                        format!("/d/{}/", target.collection_id).into_bytes()
                    };
                    let mut docs = datastore
                        .iterator(IterOptions::new().with_prefix(prefix))
                        .await
                        .map_err(Error::Storage)?;
                    let has_data = docs.next().await.map_err(Error::Storage)?.is_some();
                    docs.close().await.map_err(Error::Storage)?;
                    if has_data {
                        return Err(Error::InvalidPatch(
                            "cannot delete a collection that has documents, first delete the documents and then delete the version"
                                .to_string(),
                        ));
                    }
                }

                target.is_active = is_active;
                systemstore
                    .set(
                        &key.bytes(),
                        &serde_json::to_vec(&target).map_err(|error| {
                            Error::collection_schema_json(
                                format!("failed to serialize schema version '{}'", version_id),
                                error,
                            )
                        })?,
                    )
                    .await
                    .map_err(Error::Storage)?;

                let name_key = storage::keys::systemstore::CollectionNameKey::new(&target.name);
                if is_active {
                    let prefix = storage::keys::systemstore::CollectionVersionKey::collection_prefix(
                        &target.collection_id,
                    );
                    let mut iter = systemstore
                        .iterator(IterOptions::new().with_prefix(prefix))
                        .await
                        .map_err(Error::Storage)?;
                    let pairs = iter.collect_all().await.map_err(Error::Storage)?;
                    iter.close().await.map_err(Error::Storage)?;
                    for pair in pairs {
                        let Some(other_version_id) = String::from_utf8_lossy(&pair.key)
                            .rsplit('/')
                            .next()
                            .map(str::to_string)
                        else {
                            continue;
                        };
                        if other_version_id == version_id {
                            continue;
                        }
                        let other_key = storage::keys::systemstore::CollectionKey::new(&other_version_id);
                        let Some(other_data) = systemstore
                            .get(&other_key.bytes())
                            .await
                            .map_err(Error::Storage)?
                        else {
                            continue;
                        };
                        let Ok(mut other): std::result::Result<schema::CollectionVersion, _> =
                            serde_json::from_slice(&other_data)
                        else {
                            continue;
                        };
                        crate::chunk::ensure_admin_changeable(&other)?;
                        crate::chunk::user_writes::ensure_keeps_first_chunk_settings(
                            &target, &other,
                        )?;
                        if other.is_active {
                            other.is_active = false;
                            systemstore
                                .set(
                                    &other_key.bytes(),
                                    &serde_json::to_vec(&other).map_err(|error| {
                                        Error::collection_schema_json(
                                            "failed to serialize inactive schema version",
                                            error,
                                        )
                                    })?,
                                )
                                .await
                                .map_err(Error::Storage)?;
                        }
                    }
                    systemstore
                        .set(&name_key.bytes(), version_id.as_bytes())
                        .await
                        .map_err(Error::Storage)?;
                    let collection = Collection::load_index_actions(target.clone(), &systemstore).await?;
                    txn.cache_collection(collection);
                } else if was_active {
                    systemstore
                        .delete(&name_key.bytes())
                        .await
                        .map_err(Error::Storage)?;
                    txn.uncache_collection(&target.name);
                }
                drop(systemstore);

                let db = db.clone();
                let committed = Collection::load_index_actions(target.clone(), &txn.systemstore()?).await?;
                txn.on_commit(OnCommit::Ordered(Box::new(move || {
                    db.collections.rcu(|old| {
                        let mut cache = old.clone();
                        if committed.schema().is_active {
                            // `committed` already carries the index action state this
                            // transaction wrote; caching a plain Collection::new here
                            // would drop it the moment the callback fired.
                            cache.put_named(committed.name(), committed.clone());
                        } else if was_active {
                            cache.remove(committed.name());
                        }
                        cache
                    });
                })))?;

                Ok(target)
            })
        })
        .await
    }

    /// [`Self::delete_collections_in_txn_as`] in [`RequestContext::ambient`].
    pub async fn delete_collections_in_txn(
        &self,
        txn_id: &str,
        targets: Vec<String>,
        active_only: bool,
    ) -> Result<()> {
        self.delete_collections_in_txn_as(&RequestContext::ambient(), txn_id, targets, active_only)
            .await
    }

    pub async fn delete_collections_in_txn_as(
        &self,
        context: &RequestContext,
        txn_id: &str,
        targets: Vec<String>,
        active_only: bool,
    ) -> Result<()> {
        if targets.is_empty() {
            return Err(Error::InvalidPatch(
                "collection name required: pass at least one name to delete".into(),
            ));
        }
        self.db
            .check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await?;
        let db = self.db.clone();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                let systemstore = txn.systemstore()?;
                let mut iter = systemstore
                    .iterator(
                        IterOptions::new()
                            .with_prefix(storage::keys::systemstore::CollectionKey::collection_prefix()),
                    )
                    .await
                    .map_err(Error::Storage)?;
                let pairs = iter.collect_all().await.map_err(Error::Storage)?;
                iter.close().await.map_err(Error::Storage)?;

                let mut versions = Vec::new();
                for pair in pairs {
                    let mut version: schema::CollectionVersion = serde_json::from_slice(&pair.value)
                        .map_err(|error| {
                            Error::collection_schema_json("failed to deserialize collection", error)
                        })?;
                    crate::collection::populate_collection_root_id(&systemstore, &mut version).await?;
                    versions.push(version);
                }

                let mut deleting = rapidhash::RapidHashSet::new();
                for target in &targets {
                    let selected: Vec<&schema::CollectionVersion> = if let Some(version) = versions
                        .iter()
                        .find(|version| version.version_id == *target)
                    {
                        vec![version]
                    } else {
                        let Some(active) = versions
                            .iter()
                            .find(|version| version.name == *target && version.is_active)
                        else {
                            return Err(Error::CollectionNotFound(target.clone()));
                        };
                        if active_only {
                            vec![active]
                        } else {
                            versions
                                .iter()
                                .filter(|version| version.collection_id == active.collection_id)
                                .collect()
                        }
                    };
                    deleting.extend(
                        selected
                            .into_iter()
                            .map(|version| version.version_id.clone()),
                    );
                }

                for version in &versions {
                    if deleting.contains(&version.version_id) {
                        continue;
                    }
                    if version
                        .previous_version
                        .as_ref()
                        .is_some_and(|previous| deleting.contains(&previous.source_collection_id))
                    {
                        return Err(Error::InvalidPatch(
                            "cannot delete a version that is used by a newer version, first delete the new version"
                                .to_string(),
                        ));
                    }
                }

                let removed_collection_ids: rapidhash::RapidHashSet<String> = versions
                    .iter()
                    .filter(|version| deleting.contains(&version.version_id))
                    .filter(|version| {
                        versions.iter().all(|other| {
                            other.collection_id != version.collection_id
                                || deleting.contains(&other.version_id)
                        })
                    })
                    .map(|version| version.collection_id.clone())
                    .collect();
                if versions.iter().any(|version| {
                    !deleting.contains(&version.version_id)
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
                crate::chunk::user_writes::ensure_removed_with_parent(&versions, |version| {
                    deleting.contains(&version.version_id)
                })?;

                // Deletion writes every selected CollectionKey and active name key;
                // document writers read those same keys before staging their writes.

                let datastore = txn.datastore()?;
                for version in versions
                    .iter()
                    .filter(|version| deleting.contains(&version.version_id))
                {
                    if version.is_active && version.is_materialized {
                        let prefix = if version.query.is_some() {
                            storage::keys::datastore::ViewCacheKey::collection_prefix(version.root_id)
                        } else {
                            format!("/d/{}/", version.collection_id).into_bytes()
                        };
                        let mut docs = datastore
                            .iterator(IterOptions::new().with_prefix(prefix))
                            .await
                            .map_err(Error::Storage)?;
                        let has_data = docs.next().await.map_err(Error::Storage)?.is_some();
                        docs.close().await.map_err(Error::Storage)?;
                        if has_data {
                            return Err(Error::InvalidPatch(
                                "cannot delete a collection that has documents, first delete the documents and then delete the version"
                                    .to_string(),
                            ));
                        }
                    }
                }
                drop(datastore);

                let mut removed_names = rapidhash::RapidHashSet::new();
                for version in versions
                    .iter()
                    .filter(|version| deleting.contains(&version.version_id))
                {
                    systemstore
                        .delete(
                            &storage::keys::systemstore::CollectionKey::new(&version.version_id).bytes(),
                        )
                        .await
                        .map_err(Error::Storage)?;
                    systemstore
                        .delete(
                            &storage::keys::systemstore::CollectionVersionKey::new(
                                &version.collection_id,
                                &version.version_id,
                            )
                            .bytes(),
                        )
                        .await
                        .map_err(Error::Storage)?;
                    if version.is_active {
                        systemstore
                            .delete(
                                &storage::keys::systemstore::CollectionNameKey::new(&version.name).bytes(),
                            )
                            .await
                            .map_err(Error::Storage)?;
                        removed_names.insert(version.name.clone());
                        txn.uncache_collection(&version.name);
                    }
                }
                drop(systemstore);

                let blockstore = txn.blockstore()?;
                for version in versions
                    .iter()
                    .filter(|version| deleting.contains(&version.version_id))
                {
                    if let Ok(cid) = cid::Cid::try_from(version.version_id.as_str()) {
                        blockstore
                            .delete(&cid.to_bytes())
                            .await
                            .map_err(Error::Storage)?;
                    }
                    for field in &version.fields {
                        if let Ok(cid) = cid::Cid::try_from(field.id.as_str()) {
                            blockstore
                                .delete(&cid.to_bytes())
                                .await
                                .map_err(Error::Storage)?;
                        }
                    }
                }
                drop(blockstore);

                let db = db.clone();
                txn.on_commit(OnCommit::Ordered(Box::new(move || {
                    db.collections.rcu(|old| {
                        let mut cache = old.clone();
                        for name in &removed_names {
                            cache.remove(name);
                        }
                        cache
                    });
                    for collection_id in &removed_collection_ids {
                        let _ = db.forbid_collection_id(collection_id);
                    }
                })))?;

                Ok(())
            })
        })
        .await
    }
}
