//! Index definitions changed inside an explicit transaction. The definition
//! and its action record are staged in the caller's transaction, so the
//! transaction sees the index at once and others only after it commits. The
//! work that follows a definition change (planning and running the backfill,
//! removing a dropped index's entries) starts only once the commit is
//! durable, never for a discarded transaction, and the commit returns once it
//! is done, as the autocommit operations do.

use std::sync::Arc;

use futures::channel::oneshot;

use schema::{IndexDescription, IndexKind, IndexedFieldDescription};
use storage::corekv::{OnCommit, Store};

use crate::database::spawn::spawn_task;
use crate::error::{Error, Result};
use crate::txn::DbTxn;
use crate::DB;

impl<S> DB<S>
where
    S: Store + 'static,
{
    /// Go's `Collection.CreateIndex` in `txn`: the definition as `txn` sees
    /// the collection, and its backfill once `txn` commits.
    pub(crate) async fn create_index_in(
        self: &Arc<Self>,
        txn: &DbTxn<S>,
        collection_name: &str,
        name: Option<&str>,
        fields: Vec<IndexedFieldDescription>,
        kind: IndexKind,
    ) -> Result<IndexDescription> {
        let collection = txn
            .get_collection(collection_name)
            .await?
            .ok_or_else(|| Error::CollectionNotFound(collection_name.to_string()))?;
        txn.validate_collection_for_write(&collection).await?;
        let (index, lease) = self
            .stage_index_definition(txn, &collection, collection_name, name, fields, kind)
            .await?;
        // The rest of the transaction reads and maintains the new definition.
        txn.uncache_collection(collection_name);
        let db = Arc::clone(self);
        let (name, collection_id) = (
            collection_name.to_string(),
            collection.collection_id().to_string(),
        );
        let defined = index.clone();
        txn.on_commit(OnCommit::Async(Box::new(move || {
            Box::pin(async move {
                let plan = match db
                    .publish_index_definition(&name, &collection_id, &defined, &lease)
                    .await
                {
                    Ok(plan) => plan,
                    Err(error) => {
                        tracing::warn!(%error, index = %defined.name, "index backfill was not planned");
                        return;
                    }
                };
                // Spawned, so it finishes even if the committer stops waiting;
                // awaited, so the commit returns once the index is built, as
                // an autocommit create does.
                let (done, outcome) = oneshot::channel();
                let owner = Arc::clone(&db);
                spawn_task(owner.pool(), "db::index_backfill", async move {
                    let _ = done.send(db.backfill_index(lease, plan).await);
                });
                if let Ok(Err(error)) = outcome.await {
                    tracing::warn!(%error, "index backfill failed");
                }
            })
        })))?;
        Ok(index)
    }

    /// Go's `Collection.DropIndex` in `txn`: the index leaves the definition
    /// `txn` sees, and its entries go once `txn` commits.
    pub(crate) async fn drop_index_in(
        self: &Arc<Self>,
        txn: &DbTxn<S>,
        collection_name: &str,
        index_name: &str,
    ) -> Result<()> {
        let dropped = self
            .stage_index_drop(txn, collection_name, index_name)
            .await?;
        // The rest of the transaction reads the definition without the index.
        txn.uncache_collection(collection_name);
        let db = Arc::clone(self);
        txn.on_commit(OnCommit::Async(Box::new(move || {
            Box::pin(async move {
                db.evict_index_caches(
                    dropped.progress.collection_short_id,
                    dropped.progress.index.id,
                );
                db.publish_started_action(&dropped.lease);
                // The drop stands once its commit lands: a failed removal stays
                // on the drop record, which the next start finishes.
                let (done, outcome) = oneshot::channel();
                let owner = Arc::clone(&db);
                spawn_task(owner.pool(), "db::index_drop", async move {
                    let _ = done.send(
                        db.remove_dropped_entries(
                            dropped.lease,
                            dropped.collection_id,
                            dropped.progress,
                        )
                        .await,
                    );
                });
                let _ = outcome.await;
            })
        })))?;
        Ok(())
    }
}
