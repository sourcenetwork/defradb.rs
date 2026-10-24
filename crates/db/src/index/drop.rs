//! Dropping an index: one commit takes the index out of the definition,
//! retires its id, deletes its backfill record and writes a `DROP_INDEX`
//! record; the entries
//! are then deleted a page at a time. Nothing adds an entry once that commit
//! lands, because every writer and every backfill page reads the definition
//! it maintains the index for, so the pages need no guard. The drop record
//! carries the dropped description and the last key deleted, so a restart
//! finishes the drop. `proofs/tla/IndexDropBackfill.tla` models a drop
//! against a running backfill.

use std::sync::Arc;

use async_trait::async_trait;
use defra_core::Action;
use futures::channel::oneshot;
use schema::IndexDescription;
use serde::{Deserialize, Serialize};
use storage::corekv::{
    Key, OnCommit, RetryAttemptError, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue,
    RetryTxn, Store,
};
use storage::keys::systemstore::{ActionProgressKey, CollectionKey, CollectionNameKey};

use crate::collection::loader::load_collection_from_systemstore;
use crate::collection::Collection;
use crate::database::action::{
    action_executions, delete_action_record, stage_action_failure, ActionExecutionLease,
};
use crate::database::spawn::spawn_task;
use crate::error::{Error, Result};
use crate::index::manager::{retire_index_id, IndexType};
use crate::index::pages::{Page, PAGE_ITEMS};
use crate::txn::DbTxn;
use crate::DB;

/// What an unfinished drop resumes from: the dropped index and the last key
/// a committed page deleted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DropProgress {
    collection_short_id: u32,
    index: IndexDescription,
    after: Option<Vec<u8>>,
}

impl DropProgress {
    /// A drop of `index`, from the collection with `collection_short_id`,
    /// that has deleted every entry up to `after`.
    pub fn new(collection_short_id: u32, index: IndexDescription, after: Option<Vec<u8>>) -> Self {
        Self {
            collection_short_id,
            index,
            after,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self)
            .map_err(|error| Error::Other(format!("failed to encode index drop progress: {error}")))
    }

    /// The inverse of [`Self::encode`]; `None` for a record of another shape.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }
}

/// A drop whose definition change committed, with its entries still to go.
struct DroppedIndex {
    lease: ActionExecutionLease,
    collection_id: String,
    progress: DropProgress,
}

impl<S> DB<S>
where
    S: Store,
{
    /// Drop the index `index_name` from `collection_name` and delete its
    /// entries. The entries are deleted by a job of its own, so a caller that
    /// stops waiting does not leave them behind. Once the index is out of the
    /// definition the drop succeeds, even if deleting its entries fails.
    pub async fn drop_index(self: &Arc<Self>, collection_name: &str, index_name: &str) -> Result<()>
    where
        S: 'static,
    {
        let attempt = DropIndexAttempt {
            db: Arc::clone(self),
            collection_name: collection_name.to_string(),
            index_name: index_name.to_string(),
        };
        let dropped = match self
            .store()
            .transact_retry(self.options().retry_options(), Box::new(attempt))
            .await
        {
            Ok(value) => *value
                .downcast::<DroppedIndex>()
                .map_err(|_| Error::Other("invalid index drop result".into()))?,
            Err(error) => {
                return Err(match error.downcast_abort::<Error>() {
                    Ok(error) => error,
                    Err(error) => Error::Storage(error.into_storage()),
                })
            }
        };
        self.publish_started_action(&dropped.lease);
        let (done, outcome) = oneshot::channel();
        let db = Arc::clone(self);
        spawn_task(self.pool(), "db::index_drop", async move {
            let _ = done.send(
                db.remove_dropped_entries(dropped.lease, dropped.collection_id, dropped.progress)
                    .await,
            );
        });
        // The drop stands once its commit lands: a failed removal is logged
        // and kept on the drop record, which the next start finishes.
        let _ = outcome.await;
        Ok(())
    }

    /// Take the index out of the definition this attempt reads, delete its
    /// backfill record and write the drop record, all in `txn`.
    async fn stage_index_drop(
        &self,
        txn: &DbTxn<S>,
        collection_name: &str,
        index_name: &str,
    ) -> Result<DroppedIndex> {
        let systemstore = txn.systemstore()?;
        let collection = load_collection_from_systemstore(&systemstore, collection_name)
            .await
            .map_err(|error| Error::Other(error.to_string()))?
            .ok_or_else(|| Error::CollectionNotFound(collection_name.to_string()))?;
        let mut schema = collection.schema().clone();
        let position = schema
            .indexes
            .iter()
            .position(|index| index.name == index_name)
            .ok_or_else(|| Error::IndexNotFound {
                collection: collection_name.to_string(),
                index: index_name.to_string(),
            })?;
        let index = schema.indexes.remove(position);
        retire_index_id(&txn.datastore()?, schema.resolved_root_id(), index.id).await?;
        let data = serde_json::to_vec(&schema).map_err(|error| {
            Error::collection_schema_json(
                format!("failed to serialize schema for collection '{collection_name}'"),
                error,
            )
        })?;
        systemstore
            .set(&CollectionKey::new(&schema.version_id).bytes(), &data)
            .await?;
        systemstore
            .set(
                &CollectionNameKey::new(collection_name).bytes(),
                schema.version_id.as_bytes(),
            )
            .await?;

        let collection_id = collection.collection_id().to_string();
        let subject = index.id.to_string();
        delete_action_record(
            &systemstore,
            &collection_id,
            Action::BACKFILL_INDEX,
            &subject,
        )
        .await?;
        let lease = self
            .stage_action(&systemstore, &collection_id, Action::DROP_INDEX, &subject)
            .await?;
        let progress = DropProgress {
            collection_short_id: schema.resolved_root_id(),
            index,
            after: None,
        };
        systemstore
            .set(
                &ActionProgressKey::new(&collection_id, Action::DROP_INDEX, &subject).bytes(),
                &progress.encode()?,
            )
            .await?;

        let cached = Collection::load_index_actions(schema, &systemstore).await?;
        let cache = Arc::clone(&self.collections);
        txn.on_commit(OnCommit::Ordered(Box::new(move || {
            cache.rcu(|old| {
                let mut cache = old.clone();
                cache.put(cached.clone());
                cache
            });
        })))?;
        Ok(DroppedIndex {
            lease,
            collection_id,
            progress,
        })
    }

    /// Delete the dropped index's entries a page at a time, then its drop
    /// record. A failure stays on the record with the progress kept, so the
    /// next start picks the drop up again.
    async fn remove_dropped_entries(
        &self,
        lease: ActionExecutionLease,
        collection_id: String,
        mut progress: DropProgress,
    ) -> Result<()> {
        let mut size = PAGE_ITEMS;
        loop {
            let page = DropPage {
                collection_id: collection_id.clone(),
                progress: progress.clone(),
            };
            match self.run_page(page, &mut size).await {
                Ok(DropPageOutput { after, exhausted }) => {
                    if after.is_some() {
                        progress.after = after;
                    }
                    if exhausted {
                        break;
                    }
                }
                Err(error) => {
                    let reason = error.to_string();
                    tracing::error!(
                        collection_id = %collection_id,
                        index_id = progress.index.id,
                        %error,
                        "index drop failed"
                    );
                    let txn = self.new_txn(false).await?;
                    let systemstore = txn.systemstore()?;
                    stage_action_failure(
                        &systemstore,
                        lease.collection_id(),
                        lease.action(),
                        lease.subject(),
                        &reason,
                    )
                    .await?;
                    drop(systemstore);
                    txn.commit().await?;
                    self.publish_failed_action(&lease, &reason);
                    return Err(error);
                }
            }
        }
        self.complete_action(lease).await
    }

    /// Finish every drop a restart or a failure left unfinished, before the
    /// database takes any write.
    pub(crate) async fn resume_index_drops(&self) -> Result<()> {
        let txn = self.new_txn(true).await?;
        let mut pending = Vec::new();
        {
            let systemstore = txn.systemstore()?;
            for execution in action_executions(&systemstore).await? {
                if execution.action != Action::DROP_INDEX {
                    continue;
                }
                let progress = systemstore
                    .get(
                        &ActionProgressKey::new(
                            &execution.collection_id,
                            execution.action,
                            &execution.subject,
                        )
                        .bytes(),
                    )
                    .await
                    .map_err(Error::Storage)?
                    .and_then(|value| DropProgress::decode(&value));
                pending.push((execution, progress));
            }
        }
        txn.discard()?;

        for (execution, progress) in pending {
            let Some(progress) = progress else {
                tracing::warn!(
                    collection_id = %execution.collection_id,
                    index = %execution.subject,
                    "clearing an index drop with no progress to resume from"
                );
                self.clear_action(
                    &execution.collection_id,
                    execution.action,
                    &execution.subject,
                )
                .await?;
                continue;
            };
            let lease = self.resume_action(
                &execution.collection_id,
                execution.action,
                &execution.subject,
            )?;
            tracing::info!(
                collection_id = %execution.collection_id,
                index_id = progress.index.id,
                "resuming index drop"
            );
            // The failure is on the drop record and the next start retries
            // it, so it does not keep the database from opening.
            let _ = self
                .remove_dropped_entries(lease, execution.collection_id, progress)
                .await;
        }
        Ok(())
    }
}

/// One attempt of [`DB::drop_index`]'s definition change.
struct DropIndexAttempt<S>
where
    S: Store + 'static,
{
    db: Arc<DB<S>>,
    collection_name: String,
    index_name: String,
}

impl<S> RetryAttemptFn for DropIndexAttempt<S>
where
    S: Store + 'static,
{
    fn run<'a>(&'a mut self, retry: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let io = retry.begin_attempt_io()?;
            let txn = self
                .db
                .new_db_txn(datastore::BasicTxn::from_retry_attempt_io(io, 0));
            let dropped = self
                .db
                .stage_index_drop(&txn, &self.collection_name, &self.index_name)
                .await
                .map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(Box::new(dropped) as RetryAttemptValue)
        })
    }
}

/// One page of a drop: the entries after `progress.after`.
struct DropPage {
    collection_id: String,
    progress: DropProgress,
}

struct DropPageOutput {
    after: Option<Vec<u8>>,
    exhausted: bool,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Page for DropPage {
    type Output = DropPageOutput;

    async fn run(&mut self, txn: &mut datastore::BasicTxn, size: usize) -> Result<DropPageOutput> {
        let index = IndexType::try_new(
            self.progress.collection_short_id,
            self.progress.index.clone(),
        )?;
        let (removed, last) = index
            .remove_page(&mut txn.datastore(), self.progress.after.as_deref(), size)
            .await
            .map_err(Error::Storage)?;
        if last.is_some() {
            let progress = DropProgress {
                after: last.clone(),
                ..self.progress.clone()
            };
            txn.systemstore()
                .set(
                    &ActionProgressKey::new(
                        &self.collection_id,
                        Action::DROP_INDEX,
                        self.progress.index.id.to_string(),
                    )
                    .bytes(),
                    &progress.encode()?,
                )
                .await
                .map_err(Error::Storage)?;
        }
        Ok(DropPageOutput {
            after: last,
            exhausted: removed < size,
        })
    }
}
