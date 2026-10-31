use std::sync::Arc;

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
