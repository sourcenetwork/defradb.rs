use rapidhash::{HashSetExt, RapidHashSet};
use std::sync::Arc;

use storage::keys::{
    DataStoreKey, DatastoreSE, HeadstorePriorityKey, InstanceType, PrimaryDataStoreKey,
};

use super::*;

const CHUNK_SIZE: usize = 1000;

impl<S: Store> crate::database::DB<S> {
    /// [`Self::truncate_collection_with_filter_as`] for `identity`, or for the identity acting in [`defra_core::RequestContext::ambient`] when there is none.
    pub async fn truncate_collection_with_filter(
        self: &Arc<Self>,
        name: &str,
        filter: query::Filter,
        identity: Option<&identity::Did>,
    ) -> Result<()>
    where
        S: 'static,
    {
        self.truncate_collection_with_filter_as(
            &crate::access::nac::ambient_as(identity),
            name,
            filter,
        )
        .await
    }

    /// Permanently remove documents matching `filter` while preserving the collection.
    #[instrument(skip(self, filter), fields(collection = %name), name = "db.truncate_collection_filtered")]
    pub async fn truncate_collection_with_filter_as(
        self: &Arc<Self>,
        context: &defra_core::RequestContext,
        name: &str,
        filter: query::Filter,
    ) -> Result<()>
    where
        S: 'static,
    {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionTruncate)
            .await?;
        filter.validate_depth()?;

        let collection = self
            .get_collection(name)?
            .ok_or_else(|| Error::CollectionNotFound(name.to_string()))?;
        if collection.schema().is_branchable {
            return Err(Error::FilteredTruncateBranchableCollection);
        }
        if collection.schema().is_generated() {
            return Err(Error::TruncateGeneratedCollection(name.to_string()));
        }
        let mut generated = Vec::new();
        for chunked in &collection.schema().chunked_fields {
            let chunks_name = chunked.collection(name);
            generated.push(
                self.get_collection(&chunks_name)?
                    .ok_or(Error::CollectionNotFound(chunks_name))?,
            );
        }

        let collection_id = collection.collection_id().to_string();
        let short_id = collection.resolved_root_id();
        let action_execution = self
            .register_action(&collection_id, crate::Action::TRUNCATE)
            .await?;

        let fetcher = crate::LensedAutoCommitFetcher::new(self.clone());
        let provider = crate::DbCollectionProvider::new_arc(self.clone());
        let runner = query::QueryRunner::with_provider(fetcher, provider)
            .with_lens_store(self.lens_store().clone());

        let result: Result<usize> = async {
            let mut doc_count = 0;
            loop {
                let doc_ids = runner
                    .matching_doc_ids(name, filter.clone(), CHUNK_SIZE, true)
                    .await?;
                if doc_ids.is_empty() {
                    break;
                }
                doc_count += self
                    .truncate_filtered_chunk(&collection, &doc_ids, &generated)
                    .await?;
            }
            Ok(doc_count)
        }
        .await;

        let doc_count = match result {
            Ok(doc_count) => doc_count,
            Err(error) => {
                if let Err(action_error) =
                    self.fail_action(action_execution, &error.to_string()).await
                {
                    tracing::error!(
                        error = %action_error,
                        collection_id = %collection_id,
                        "Failed to record filtered truncate action error"
                    );
                }
                return Err(error);
            }
        };

        self.complete_action(action_execution).await?;
        tracing::info!(
            collection_id = %collection_id,
            short_id,
            doc_count,
            "Truncated matching collection documents"
        );
        Ok(())
    }

    // Includes old chunks, which the parents' lists no longer name.
    async fn chunk_doc_ids(
        txn: &crate::txn::DbTxn<S>,
        chunks_col: &Collection,
        parents: &[String],
    ) -> Result<Vec<String>> {
        use storage::index::IndexIterator;

        let index_manager = crate::IndexManager::from_indexes(
            chunks_col.resolved_root_id(),
            chunks_col.schema(),
            chunks_col.write_indexes(),
        )?;
        let index = chunks_col
            .schema()
            .indexes
            .iter()
            .find(|index| {
                index
                    .fields
                    .first()
                    .is_some_and(|field| field.name == schema::CHUNK_SOURCE_ID_FIELD)
            })
            .and_then(|index| index_manager.get_index(&index.name))
            .ok_or_else(|| Error::Other(format!("{} has no source index", chunks_col.name())))?;
        let datastore = txn.datastore()?;
        let systemstore = txn.systemstore()?;
        let mut doc_ids = Vec::new();
        for parent in parents {
            let mut entries = index
                .scan_prefix(
                    &datastore,
                    &[document::NormalValue::String(parent.clone())],
                    false,
                )
                .await
                .map_err(Error::Storage)?;
            for entry in entries.collect_all().await.map_err(Error::Storage)? {
                if let Some(doc_id) =
                    crate::docid::map::get_doc_id(&systemstore, entry.doc_short_id).await?
                {
                    doc_ids.push(doc_id);
                }
            }
        }
        Ok(doc_ids)
    }

    // One transaction per page, so a parent and its `@chunk` chunks go together.
    // A conflict with a concurrent write runs the page again.
    async fn truncate_filtered_chunk(
        self: &Arc<Self>,
        collection: &Collection,
        doc_ids: &[String],
        generated: &[Collection],
    ) -> Result<usize>
    where
        S: 'static,
    {
        let attempt = TruncatePage {
            db: Arc::clone(self),
            collection: collection.clone(),
            doc_ids: doc_ids.to_vec(),
            generated: generated.to_vec(),
        };
        match self
            .store()
            .transact_retry(self.options().retry_options(), Box::new(attempt))
            .await
        {
            Ok(count) => count
                .downcast::<usize>()
                .map(|count| *count)
                .map_err(|_| Error::Other("invalid truncate page result".into())),
            Err(error) => Err(match error.downcast_abort::<Error>() {
                Ok(error) => error,
                Err(error) => Error::Storage(error.into_storage()),
            }),
        }
    }

    async fn remove_documents(
        txn: &crate::txn::DbTxn<S>,
        collection: &Collection,
        doc_ids: &[String],
    ) -> Result<usize> {
        let datastore = txn.datastore()?;
        let headstore = txn.headstore()?;
        let blockstore = txn.blockstore()?;
        let systemstore = txn.systemstore()?;

        let result: Result<usize> = async {
            let index_manager = crate::IndexManager::from_indexes(
                collection.resolved_root_id(),
                collection.schema(),
                collection.write_indexes(),
            )?;
            let mut seen = RapidHashSet::new();
            let mut targets = Vec::with_capacity(doc_ids.len());
            let mut aliases_to_delete = RapidHashSet::new();

            for doc_id in doc_ids {
                let parsed = document::DocID::from_string(doc_id)?;
                let (doc_short_id, canonical_doc_id) = collection
                    .resolve_doc_identity(&systemstore, &parsed)
                    .await?
                    .ok_or_else(|| Error::DocumentNotFound(doc_id.clone()))?;
                if !seen.insert(doc_short_id) {
                    continue;
                }

                let mut aliases =
                    crate::docid::map::get_doc_id_aliases(&systemstore, doc_short_id).await?;
                if !aliases
                    .iter()
                    .any(|alias| alias == &canonical_doc_id.to_string())
                {
                    aliases.push(canonical_doc_id.to_string());
                }
                aliases_to_delete.extend(aliases.iter().cloned());
                targets.push((doc_short_id, canonical_doc_id, aliases));
            }

            let se_prefix = DatastoreSE::collection_prefix(collection.collection_id());
            let mut se_iter = datastore
                .iterator(IterOptions::new().with_prefix(se_prefix))
                .await
                .map_err(Error::Storage)?;
            let mut se_keys = Vec::new();
            while let Some(pair) = se_iter.next().await.map_err(Error::Storage)? {
                if extract_last_path_segment_str(&pair.key)
                    .is_some_and(|doc_id| aliases_to_delete.contains(&doc_id))
                {
                    se_keys.push(pair.key.to_vec());
                }
            }
            se_iter.close().await.map_err(Error::Storage)?;
            for key in se_keys {
                datastore.delete(&key).await.map_err(Error::Storage)?;
            }

            for (doc_short_id, canonical_doc_id, aliases) in &targets {
                if let Some((doc, _)) = collection
                    .get_with_datastore_include_deleted(
                        &datastore,
                        *doc_short_id,
                        canonical_doc_id,
                        true,
                    )
                    .await?
                {
                    index_manager
                        .on_document_delete(&datastore, &doc, *doc_short_id, collection.schema())
                        .await?;
                }

                datastore
                    .delete(&collection.doc_key(*doc_short_id))
                    .await
                    .map_err(Error::Storage)?;
                datastore
                    .delete(&collection.deleted_key(*doc_short_id))
                    .await
                    .map_err(Error::Storage)?;
                datastore
                    .delete(&collection.version_key(*doc_short_id))
                    .await
                    .map_err(Error::Storage)?;
                datastore
                    .delete(
                        &PrimaryDataStoreKey::new(collection.resolved_root_id(), *doc_short_id)
                            .bytes(),
                    )
                    .await
                    .map_err(Error::Storage)?;
                for instance_type in [
                    InstanceType::Value,
                    InstanceType::Priority,
                    InstanceType::Deleted,
                ] {
                    delete_prefix(
                        &datastore,
                        DataStoreKey::document_prefix(
                            collection.resolved_root_id(),
                            instance_type,
                            *doc_short_id,
                        ),
                    )
                    .await?;
                }

                let heads = crate::block::heads::document::load(&headstore, *doc_short_id)
                    .await
                    .map_err(Error::Storage)?;
                let block_cids = crate::block::heads::document::live_cids(&heads);
                headstore
                    .delete(&crate::block::heads::document::set_key(*doc_short_id))
                    .await
                    .map_err(Error::Storage)?;
                delete_prefix(
                    &headstore,
                    HeadstorePriorityKey::document_prefix(*doc_short_id),
                )
                .await?;

                crate::block::cleanup::delete_owned_dag_for_owners(
                    &blockstore,
                    &systemstore,
                    &block_cids,
                    aliases,
                )
                .await?;
                crate::docid::map::delete_doc_id_mappings(&systemstore, *doc_short_id).await?;
            }

            Ok(targets.len())
        }
        .await;

        result
    }
}

struct TruncatePage<S>
where
    S: Store,
{
    db: Arc<crate::database::DB<S>>,
    collection: Collection,
    doc_ids: Vec<String>,
    generated: Vec<Collection>,
}

impl<S> storage::corekv::RetryAttemptFn for TruncatePage<S>
where
    S: Store + 'static,
{
    fn run<'a>(
        &'a mut self,
        retry: &'a mut dyn storage::corekv::RetryTxn,
    ) -> storage::corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            let abort = |error: Error| storage::corekv::RetryAttemptError::Abort(Box::new(error));
            let io = retry.begin_attempt_io()?;
            let txn = self
                .db
                .new_db_txn(datastore::BasicTxn::from_retry_attempt_io(
                    io,
                    0,
                    self.db.crdts().clone(),
                ));
            let count = async {
                txn.validate_collection_for_write(&self.collection).await?;
                let count =
                    crate::database::DB::remove_documents(&txn, &self.collection, &self.doc_ids)
                        .await?;
                for chunks_col in &self.generated {
                    txn.validate_collection_for_write(chunks_col).await?;
                    let chunk_ids =
                        crate::database::DB::chunk_doc_ids(&txn, chunks_col, &self.doc_ids).await?;
                    crate::database::DB::remove_documents(&txn, chunks_col, &chunk_ids).await?;
                }
                Ok::<_, Error>(count)
            }
            .await
            .map_err(abort)?;
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(Box::new(count) as storage::corekv::RetryAttemptValue)
        })
    }
}
