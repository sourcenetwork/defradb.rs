use std::sync::Arc;

use futures::StreamExt;

use super::*;

const CHUNK_SIZE: usize = 1000;

/// Documents purged at once, each in its own attempt.
const IN_FLIGHT: usize = 64;

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

    /// Purge one chunk, each document in its own engine-retried attempt: a
    /// conflict, such as a write to the document or a merge adopting a block
    /// the owner scan decided on, re-runs only that document.
    async fn truncate_filtered_chunk(
        self: &Arc<Self>,
        collection: &Collection,
        doc_ids: &[String],
    ) -> Result<usize>
    where
        S: 'static,
    {
        let doc_ids = doc_ids
            .iter()
            .map(|doc_id| document::DocID::from_string(doc_id))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut purges = futures::stream::iter(doc_ids)
            .map(|doc_id| self.truncate_filtered_doc(collection, doc_id))
            .buffer_unordered(IN_FLIGHT);
        let mut purged = 0;
        while let Some(count) = purges.next().await {
            purged += count?;
        }
        Ok(purged)
    }

    async fn truncate_filtered_doc(
        self: &Arc<Self>,
        collection: &Collection,
        doc_id: document::DocID,
    ) -> Result<usize>
    where
        S: 'static,
    {
        let db = Arc::clone(self);
        let collection = collection.clone();
        self.transact_db(move |txn| {
            let db = Arc::clone(&db);
            let collection = collection.clone();
            let doc_id = doc_id.clone();
            Box::pin(async move {
                txn.validate_collection_for_write(&collection).await?;
                db.purge_documents(txn, &collection, std::slice::from_ref(&doc_id))
                    .await
            })
        })
        .await
    }
}
