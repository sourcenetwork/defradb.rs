//! Views added and refreshed inside a transaction. Each checks node access
//! in the transaction, with the permissions its autocommit form needs.

use super::*;
use acp::nac::NodePermission;
use storage::corekv::OnCommit;

impl<S: Store + 'static> DbTransactionRegistry<S> {
    /// Go's `AddView` in transaction `txn_id`: the view collections are
    /// created in the transaction, visible in it at once and to others once
    /// it commits. Only then are the materialized views in `materialized`
    /// built and the downsamples in `downsample` bootstrapped, before the
    /// commit returns, as an autocommit `AddView` builds them before it does.
    /// It needs `add-view` and `patch-collection`, as the autocommit form
    /// does, and a multi-type SDL that fails part way leaves nothing.
    #[allow(clippy::too_many_arguments)]
    pub async fn add_view_in_txn(
        &self,
        context: &RequestContext,
        txn_id: &str,
        collections: Vec<schema::CollectionVersion>,
        document_acp: Option<Arc<dyn acp::DocumentACP>>,
        creator: Option<identity::Did>,
        materialized: Vec<String>,
        downsample: Vec<String>,
    ) -> Result<Vec<schema::CollectionVersion>> {
        let db = self.db.clone();
        let request = context.clone();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                for permission in [NodePermission::ViewAdd, NodePermission::CollectionPatch] {
                    db.check_node_access_in_txn(&request, txn.acpstore()?, permission)
                        .await?;
                }
                txn.set_statement_savepoint()?;
                let view = StagedView {
                    collections,
                    document_acp,
                    creator,
                    materialized,
                    downsample,
                };
                let created = view.stage(&db, txn, &request).await;
                finish_statement(txn, created)
            })
        })
        .await
    }

    /// Go's `RefreshViews` in transaction `txn_id`. The views `selector`
    /// names are rebuilt once the transaction commits, from what it
    /// committed, before the commit returns; a discarded transaction
    /// rebuilds nothing. A selector the refresh would refuse is refused
    /// here, against the views the transaction sees, and a read-only
    /// transaction may not ask for the rebuild's writes.
    pub async fn refresh_views_in_txn(
        &self,
        context: &RequestContext,
        txn_id: &str,
        selector: schema::CollectionSelector,
    ) -> Result<()> {
        let db = self.db.clone();
        let request = context.clone();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let txn = owned.alone()?;
                db.check_node_access_in_txn(&request, txn.acpstore()?, NodePermission::ViewRefresh)
                    .await?;
                if txn.is_readonly()? {
                    return Err(Error::Storage(storage::Error::ReadOnlyTxn));
                }
                let versions: Vec<_> = db
                    .get_all_collection_versions_with_txn(txn)
                    .await?
                    .into_iter()
                    .filter(|version| selector.needs_all_versions() || version.is_active)
                    .collect();
                crate::view::ops::refresh_targets(&versions, &selector)?;
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

/// An AddView's view collections and the work that follows their commit.
struct StagedView {
    collections: Vec<schema::CollectionVersion>,
    document_acp: Option<Arc<dyn acp::DocumentACP>>,
    creator: Option<identity::Did>,
    materialized: Vec<String>,
    downsample: Vec<String>,
}

impl StagedView {
    /// Stage the view collections on `txn`, lift their forbidden IDs in the
    /// commit's order, and build and bootstrap them once the commit is
    /// durable.
    async fn stage<S>(
        self,
        db: &Arc<DB<S>>,
        txn: &mut crate::txn::DbTxn<S>,
        request: &RequestContext,
    ) -> Result<Vec<schema::CollectionVersion>>
    where
        S: Store + 'static,
    {
        let created = db
            .stage_new_collections(
                txn,
                request,
                self.collections,
                self.document_acp,
                self.creator,
            )
            .await?;
        let ids: Vec<String> = created.iter().map(|c| c.collection_id.clone()).collect();
        let lifted = Arc::clone(db);
        txn.on_commit(OnCommit::Ordered(Box::new(move || {
            for id in &ids {
                let _ = lifted.unforbid_collection_id(id);
            }
        })))?;
        let (db, materialized, downsample) = (Arc::clone(db), self.materialized, self.downsample);
        // The bootstrap runs after the transaction has ended.
        let after = request.clone().with_txn(None);
        txn.on_commit(OnCommit::Async(Box::new(move || {
            Box::pin(async move {
                if !materialized.is_empty() {
                    let selector = schema::CollectionSelector::with_names(materialized);
                    if let Err(error) = db.refresh_views(selector).await {
                        tracing::warn!(%error, "view build after commit failed");
                    }
                }
                if !downsample.is_empty() {
                    if let Err(error) = db.bootstrap_downsamples_as(&after, Some(&downsample)).await
                    {
                        tracing::warn!(%error, "downsample bootstrap after commit failed");
                    }
                }
            })
        })))?;
        Ok(created)
    }
}
