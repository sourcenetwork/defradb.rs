//! Views added and refreshed inside a transaction.

use super::*;
use storage::corekv::OnCommit;

impl<S: Store + 'static> DbTransactionRegistry<S> {
    /// Go's `AddView` in transaction `txn_id`: the view collections are
    /// created in the transaction, visible in it at once and to others once
    /// it commits. Only then are the materialized views in `materialized`
    /// built and the downsamples in `downsample` bootstrapped, before the
    /// commit returns, as an autocommit `AddView` builds them before it does.
    #[allow(clippy::too_many_arguments)]
    pub async fn add_view_in_txn(
        &self,
        context: &RequestContext,
        txn_id: &str,
        collections: Vec<schema::CollectionVersion>,
        document_acp: Arc<dyn acp::DocumentACP>,
        creator: Option<identity::Did>,
        materialized: Vec<String>,
        downsample: Vec<String>,
    ) -> Result<Vec<schema::CollectionVersion>> {
        let db = self.db.clone();
        let request = context.clone();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                let created = db
                    .stage_new_collections(txn, &request, collections, Some(document_acp), creator)
                    .await?;
                let ids: Vec<String> = created.iter().map(|c| c.collection_id.clone()).collect();
                txn.on_commit(OnCommit::Async(Box::new(move || {
                    Box::pin(async move {
                        for id in &ids {
                            let _ = db.unforbid_collection_id(id);
                        }
                        if !materialized.is_empty() {
                            let selector = schema::CollectionSelector::with_names(materialized);
                            if let Err(error) = db.refresh_views(selector).await {
                                tracing::warn!(%error, "view build after commit failed");
                            }
                        }
                        if !downsample.is_empty() {
                            if let Err(error) = db
                                .bootstrap_downsamples_as(&request, Some(&downsample))
                                .await
                            {
                                tracing::warn!(%error, "downsample bootstrap after commit failed");
                            }
                        }
                    })
                })))?;
                Ok(created)
            })
        })
        .await
    }

    /// Go's `RefreshViews` in transaction `txn_id`. The views `selector`
    /// names are rebuilt once the transaction commits, from what it
    /// committed, before the commit returns; a discarded transaction
    /// rebuilds nothing.
    pub async fn refresh_views_in_txn(
        &self,
        context: &RequestContext,
        txn_id: &str,
        selector: schema::CollectionSelector,
    ) -> Result<()> {
        let db = self.db.clone();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                txn.on_commit(OnCommit::Async(Box::new(move || {
                    Box::pin(async move {
                        if let Err(error) = db.refresh_views(selector).await {
                            tracing::warn!(%error, "view refresh after commit failed");
                        }
                    })
                })))?;
                Ok(())
            })
        })
        .await
    }
}
