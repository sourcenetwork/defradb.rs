use rapidhash::{HashSetExt, RapidHashSet};
use std::sync::Arc;

use storage::keys::{
    DataStoreKey, DatastoreSE, HeadstorePriorityKey, InstanceType, PrimaryDataStoreKey,
};

use super::*;

const CHUNK_SIZE: usize = 1000;

impl<S: Store> crate::database::DB<S> {
    /// Delete the documents of `collection` that `filter` matches, with their
    /// unshared history, keeping the collection; the number of documents.
    pub(super) async fn truncate_matching(
        self: &Arc<Self>,
        collection: &Collection,
        filter: &query::Filter,
    ) -> Result<usize>
    where
        S: 'static,
    {
        let fetcher = crate::LensedAutoCommitFetcher::new(self.clone());
        let provider = crate::DbCollectionProvider::new_arc(self.clone());
        let runner = query::QueryRunner::with_provider(fetcher, provider)
            .with_lens_store(self.lens_store().clone());

        let mut doc_count = 0;
        loop {
            let doc_ids = runner
                .matching_doc_ids(collection.name(), filter.clone(), CHUNK_SIZE, true)
                .await?;
            if doc_ids.is_empty() {
                return Ok(doc_count);
            }
            doc_count += self.truncate_filtered_chunk(collection, &doc_ids).await?;
        }
    }

    async fn truncate_filtered_chunk(
        &self,
        collection: &Collection,
        doc_ids: &[String],
    ) -> Result<usize> {
        let txn = self.new_txn(false).await?;
        txn.validate_collection_for_write(collection).await?;
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

        drop(datastore);
        drop(headstore);
        drop(blockstore);
        drop(systemstore);

        match result {
            Ok(count) => {
                txn.commit().await?;
                Ok(count)
            }
            Err(error) => {
                if let Err(discard_err) = txn.discard() {
                    tracing::warn!(
                        error = %discard_err,
                        original_error = %error,
                        "Transaction discard failed during filtered truncate"
                    );
                }
                Err(error)
            }
        }
    }
}
