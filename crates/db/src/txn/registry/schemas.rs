//! Schema and migration mutations inside a transaction.

use super::*;
use storage::corekv::OnCommit;

impl<S: Store + 'static> DbTransactionRegistry<S> {
    /// [`Self::set_migration_in_txn_as`] for `identity`, or for the identity
    /// acting in [`RequestContext::ambient`] when there is none.
    pub async fn set_migration_in_txn(
        &self,
        txn_id: &str,
        config: lens::LensConfig,
        identity: Option<&identity::Did>,
    ) -> Result<lens::TransformId> {
        self.set_migration_in_txn_as(&crate::access::nac::ambient_as(identity), txn_id, config)
            .await
    }

    /// Set a migration within an existing transaction.
    ///
    /// This registers a lens migration configuration within the specified transaction.
    /// The migration will only be visible after the transaction is committed.
    ///
    /// # Arguments
    ///
    /// * `txn_id` - The transaction ID from `begin_txn`
    /// * `config` - The lens configuration
    ///
    /// # Returns
    ///
    /// The transform ID that was registered.
    pub async fn set_migration_in_txn_as(
        &self,
        context: &RequestContext,
        txn_id: &str,
        config: lens::LensConfig,
    ) -> Result<lens::TransformId> {
        self.db
            .check_node_access_as(context, acp::nac::NodePermission::MigrationSet)
            .await?;
        let db = self.db.clone();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn_lens_store = owned.lens_store();
                let caches = owned.caches();
                let txn = owned.alone()?;

                let outcome = db
                    .set_migration_in_txn_with_store(txn, txn_lens_store.clone(), config.clone())
                    .await?;
                let transform_id = outcome.transform_id.clone();
                let updated_destination = outcome.updated_destination.clone();
                let updated_collection = Collection::load_index_actions(
                    updated_destination.clone(),
                    &txn.systemstore()?,
                )
                .await?;
                // The migration changed what this transaction's fetchers
                // derived from its schema.
                caches.invalidate();

                let db = db.clone();
                let cache_db = db.clone();
                let cache_destination = updated_destination;
                let cache_collection = updated_collection.clone();
                txn.on_commit(OnCommit::Ordered(Box::new(move || {
                    if cache_destination.name.is_empty() {
                        return;
                    }
                    cache_db.collections.rcu(|old| {
                        let mut cache = old.clone();
                        let matches_destination =
                            cache.get(&cache_destination.name).is_some_and(|cached| {
                                cached.schema().version_id == cache_destination.version_id
                            });
                        if matches_destination {
                            cache.put_named(&cache_destination.name, cache_collection.clone());
                        }
                        cache
                    });
                })))?;
                let transform_id_for_commit = transform_id.clone();
                txn.on_commit(OnCommit::Async(Box::new(move || {
                    let db = db.clone();
                    let config = config.clone();
                    let transform_id = transform_id_for_commit.clone();
                    Box::pin(async move {
                        db.bump_migration_generation();
                        db.register_committed_lens(transform_id, config).await;
                        db.settle_owed_reindexes_after_commit().await;
                    })
                })))?;

                Ok(transform_id)
            })
        })
        .await
    }

    /// [`Self::add_collection_in_txn_as`] in [`RequestContext::ambient`].
    pub async fn add_collection_in_txn(
        &self,
        txn_id: &str,
        sdl: &str,
    ) -> Result<Vec<schema::CollectionVersion>> {
        self.add_collection_in_txn_as(&RequestContext::ambient(), txn_id, sdl)
            .await
    }

    /// Add a schema within an existing transaction, through the same checks
    /// as [`crate::database::DB::add_collection`]. The collections are only
    /// visible after the transaction commits, but queries within it can use
    /// them. Branchable collection ACP objects are registered before the
    /// storage commit when document ACP is installed.
    pub async fn add_collection_in_txn_as(
        &self,
        context: &RequestContext,
        txn_id: &str,
        sdl: &str,
    ) -> Result<Vec<schema::CollectionVersion>> {
        let to_error = |e: client::Error| Error::Other(e.to_string());
        self.db
            .check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await?;
        let creator = DB::<S>::creator_of(context).map_err(to_error)?;
        let db = self.db.clone();
        let request = context.clone();
        let sdl = sdl.to_owned();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;

                let existing = db.get_all_collection_versions_with_txn(txn).await?;
                let mut known_types: rapidhash::RapidHashSet<String> = db
                    .list_collections()
                    .map_err(|e| Error::Other(format!("failed to list collections: {}", e)))?
                    .into_iter()
                    .collect();
                known_types.extend(existing.iter().map(|c| c.name.clone()));

                let collections = db
                    .prepare_new_collections(&sdl, known_types)
                    .await
                    .map_err(to_error)?;
                schema::definition_validation::validate_new_collections_with_existing(
                    &collections,
                    &existing,
                )
                .map_err(Error::Other)?;

                let mut finalized = Vec::new();
                for collection in collections {
                    let schema = db.create_collection_with_txn(txn, collection).await?;
                    finalized.push(schema);
                }

                if let (Some(document_acp), Some(creator)) = (db.document_acp().cloned(), creator) {
                    txn.stage_collection_acp_registration(
                        &request,
                        document_acp,
                        creator,
                        finalized.clone(),
                    );
                }

                // Register on_success callback to update the process-wide cache after commit
                let db = db.clone();
                let schemas_for_cache = finalized.clone();
                let mut collections_for_cache = Vec::with_capacity(finalized.len());
                for schema in &finalized {
                    collections_for_cache.push(
                        Collection::load_index_actions(schema.clone(), &txn.systemstore()?).await?,
                    );
                }
                txn.on_commit(OnCommit::Ordered(Box::new(move || {
                    for schema in &schemas_for_cache {
                        let _ = db.unforbid_collection_id(&schema.collection_id);
                    }
                    db.collections.rcu(|old| {
                        let mut cache = old.clone();
                        for collection in &collections_for_cache {
                            cache.put(collection.clone());
                        }
                        cache
                    });
                })))?;

                Ok(finalized)
            })
        })
        .await
    }
}
