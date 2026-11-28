//! Collection patches inside a transaction.

use super::*;
use storage::corekv::OnCommit;

impl<S> DbTransactionRegistry<S>
where
    S: Store + 'static,
{
    /// Apply a collection patch within an existing transaction, through the
    /// same checks and writes as [`DB::patch_collection`]. The patched
    /// definitions are visible to this transaction alone until it commits,
    /// and gone if it is discarded; a patch that fails leaves nothing in it.
    /// The process-wide collection cache, the lenses the patch committed and
    /// the reindexes it owes follow the commit.
    pub async fn patch_collection_in_txn_as(
        &self,
        context: &RequestContext,
        txn_id: &str,
        patch: &str,
        migration: Option<lens::LensConfig>,
    ) -> Result<()> {
        let raw = crate::definition::patch::validate_operations(patch)?;
        let db = self.db.clone();
        let request = context.clone();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let lens_store = owned.lens_store();
                let caches = owned.caches();
                let txn = owned.alone()?;
                db.check_node_access_in_txn(
                    &request,
                    txn.acpstore()?,
                    acp::nac::NodePermission::CollectionPatch,
                )
                .await?;
                owned.begin_operation_statement()?;
                let txn = owned.alone()?;
                let commit_token = txn.commit_token();
                let patched = db
                    .patch_collection_with_txn(txn, &raw, migration, lens_store)
                    .await;
                let prepared = patched.and_then(|patched| {
                    crate::definition::patch::forget_cached_collections(txn);
                    // The patch changed what this transaction's fetchers derived
                    // from its schema.
                    caches.invalidate();
                    txn.on_commit(OnCommit::Async(Box::new(move || {
                        Box::pin(async move {
                            let commit = commit_token.load_clone();
                            if let Err(error) = db.after_patch(patched, commit).await {
                                tracing::error!(%error, "follow-up to a committed collection patch failed");
                            }
                        })
                    })))?;
                    Ok(())
                });
                finish_statement(owned, prepared)
            })
        })
        .await
    }
}
