use std::sync::Arc;

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
                doc_count += self.truncate_filtered_chunk(&collection, &doc_ids).await?;
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

    async fn truncate_filtered_chunk(
        &self,
        collection: &Collection,
        doc_ids: &[String],
    ) -> Result<usize> {
        let doc_ids = doc_ids
            .iter()
            .map(|doc_id| document::DocID::from_string(doc_id))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let txn = self.new_txn(false).await?;
        txn.validate_collection_for_write(collection).await?;
        match self.purge_documents(&txn, collection, &doc_ids, true).await {
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
