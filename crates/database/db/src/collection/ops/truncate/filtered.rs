use std::sync::Arc;

use crate::collection::provider::TxnCollectionProvider;
use crate::read::lensed::fetcher::LensedDocFetcher;
use crate::txn::LentTxn;
use query::DocFetcher;

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
        let mut doc_count = 0;
        loop {
            let purged = self
                .truncate_filtered_chunk(collection, filter, CHUNK_SIZE)
                .await?;
            if purged == 0 {
                return Ok(doc_count);
            }
            doc_count += purged;
        }
    }

    /// Purge one page in an engine-retried transaction. The page's matches are
    /// chosen in that attempt with validated scans, so a filter-moving write
    /// conflicts and the retry chooses the matches afresh.
    async fn truncate_filtered_chunk(
        self: &Arc<Self>,
        collection: &Collection,
        filter: &query::Filter,
        limit: usize,
    ) -> Result<usize>
    where
        S: 'static,
    {
        let db = Arc::clone(self);
        let collection = collection.clone();
        let filter = filter.clone();
        self.transact_db_with_txn(move |txn| {
            let db = Arc::clone(&db);
            let collection = collection.clone();
            let filter = filter.clone();
            Box::pin(async move {
                txn.validate_collection_for_write(&collection).await?;
                let doc_ids =
                    matching_doc_ids_in_attempt(&db, txn, &collection, &filter, limit).await?;
                let doc_ids = doc_ids
                    .iter()
                    .map(|doc_id| document::DocID::from_string(doc_id))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                let purged = db.purge_documents(txn, &collection, &doc_ids).await?;
                Ok((purged, txn.clone()))
            })
        })
        .await
    }
}

/// The documents `filter` matches in `txn`, with every scan this decision
/// reads validated: a concurrent write that can move a document across the
/// filter conflicts with the attempt.
async fn matching_doc_ids_in_attempt<S>(
    db: &Arc<crate::database::DB<S>>,
    txn: &LentTxn<S>,
    collection: &Collection,
    filter: &query::Filter,
    limit: usize,
) -> Result<Vec<String>>
where
    S: Store + 'static,
{
    let fetcher = Arc::new(LensedDocFetcher::lent(
        db.clone(),
        txn,
        db.lens_store().clone(),
        Default::default(),
    ));
    fetcher.set_scans_validated(true);
    let provider = Arc::new(TxnCollectionProvider::new(db.clone(), txn));
    let runner = query::QueryRunner::with_provider(fetcher.clone(), provider)
        .with_lens_store(db.lens_store().clone());

    let found = runner
        .matching_doc_ids(collection.name(), filter.clone(), limit, true)
        .await;
    drop(runner);
    fetcher.set_scans_validated(false);
    Ok(found?)
}
