//! How an index backfill ends: one commit that settles its action record as
//! completed or failed and publishes the index's new state to the collection
//! cache before that commit returns.
//!
//! The planner reads only the cache, so a settlement that reached the store
//! but not the cache would leave a built index unused, or a failed one still
//! maintained by writers. The publication runs on the settling commit, in
//! commit order, and changes only its own index's status in the cache entry it
//! finds there, so another index's settlement cannot be overtaken
//! (`proofs/tla/IndexCachePublication.tla`).

use std::sync::Arc;

use datastore::NamespaceView;
use defra_core::{Action, ActionExecution};
use storage::corekv::{
    Key, OnCommit, RetryAttemptError, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue,
    RetryTxn,
};

use crate::collection::epoch::EpochedCollections;
use crate::collection::Collection;
use crate::database::action::{delete_action_record, stage_action_failure};
use crate::error::{Error, Result};
use crate::index::backfill::{ensure_backfill_index_unchanged, BackfillPlan};
use crate::index::IndexManager;

/// Publish `collection`'s definition and the transaction's index `changes` to
/// the cache once that transaction commits, before the commit returns.
async fn publish_on_commit(
    systemstore: &NamespaceView,
    cache: &Arc<EpochedCollections>,
    collection_id: &str,
    changes: Vec<(u32, Option<defra_core::ActionStatus>)>,
    event_bus: Option<Arc<dyn events::Bus>>,
    execution: ActionExecution,
) -> Result<()> {
    let collection_id = collection_id.to_string();
    let cache = Arc::clone(cache);
    systemstore
        .on_commit(OnCommit::OrderedAndWait(Box::new(move || {
            cache.publish_statuses(&collection_id, &changes);
            if let Some(bus) = event_bus {
                bus.publish(events::Message::action_execution(execution));
            }
        })))
        .await
        .map_err(Error::Storage)
}

/// Publish the completed index: its last build step, its action record
/// cleared, and the cache told it is built, in one commit against the same
/// definition snapshot as its build.
pub(super) struct FinishIndexAttempt {
    pub(super) collection: Collection,
    pub(super) plan: BackfillPlan,
    pub(super) cache: Arc<EpochedCollections>,
    pub(super) event_bus: Option<Arc<dyn events::Bus>>,
    pub(super) execution: ActionExecution,
}

impl RetryAttemptFn for FinishIndexAttempt {
    fn run<'a>(&'a mut self, retry: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let io = retry.begin_attempt_io()?;
            let txn =
                datastore::BasicTxn::from_retry_attempt_io(io, 0, self.collection.crdts().clone());
            let datastore = txn.datastore();
            let systemstore = txn.systemstore();
            let built: Result<()> = async move {
                let collection = crate::collection::loader::load_collection_for_write_attempt(
                    &systemstore,
                    self.collection.name(),
                    &self.collection,
                )
                .await?;
                ensure_backfill_index_unchanged(&self.plan, &collection)?;
                let manager = IndexManager::from_collection(
                    collection.schema().resolved_root_id(),
                    collection.schema(),
                    collection.crdts(),
                )?;
                manager
                    .build_index(&datastore, self.plan.index_name())
                    .await?;
                delete_action_record(
                    &systemstore,
                    self.plan.collection_id(),
                    Action::BACKFILL_INDEX,
                    &self.plan.index_id().to_string(),
                )
                .await?;
                publish_on_commit(
                    &systemstore,
                    &self.cache,
                    collection.collection_id(),
                    vec![(self.plan.index_id(), None)],
                    self.event_bus.clone(),
                    self.execution.clone(),
                )
                .await
            }
            .await;
            built.map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(Box::new(()) as RetryAttemptValue)
        })
    }
}

/// Settle a failed backfill against the definition the attempt reads: the
/// record errored and the cache told so, unless the index is gone.
pub(super) struct FailBackfillAttempt {
    pub(super) plan: BackfillPlan,
    pub(super) reason: String,
    pub(super) crdts: storage::Crdts,
    pub(super) cache: Arc<EpochedCollections>,
    pub(super) event_bus: Option<Arc<dyn events::Bus>>,
    pub(super) execution: ActionExecution,
}

impl RetryAttemptFn for FailBackfillAttempt {
    fn run<'a>(&'a mut self, retry: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let io = retry.begin_attempt_io()?;
            let txn = datastore::BasicTxn::from_retry_attempt_io(io, 0, self.crdts.clone());
            let systemstore = txn.systemstore();
            let recorded: Result<bool> = async {
                let collection = crate::collection::loader::load_collection_from_systemstore(
                    &systemstore,
                    self.plan.collection_name(),
                )
                .await
                .map_err(|error| Error::Other(error.to_string()))?
                .filter(|collection| {
                    collection.collection_id() == self.plan.collection_id()
                        && collection
                            .schema()
                            .indexes
                            .iter()
                            .any(|index| index.id == self.plan.index_id())
                });
                let Some(collection) = collection else {
                    return Ok(false);
                };
                stage_action_failure(
                    &systemstore,
                    self.plan.collection_id(),
                    Action::BACKFILL_INDEX,
                    &self.plan.index_id().to_string(),
                    &self.reason,
                )
                .await?;
                systemstore
                    .delete(&self.plan.progress_key().bytes())
                    .await
                    .map_err(Error::Storage)?;
                publish_on_commit(
                    &systemstore,
                    &self.cache,
                    collection.collection_id(),
                    vec![(
                        self.plan.index_id(),
                        Some(defra_core::ActionStatus::ERRORED),
                    )],
                    self.event_bus.clone(),
                    self.execution.clone(),
                )
                .await?;
                Ok(true)
            }
            .await;
            let recorded = recorded.map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
            drop(systemstore);
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(Box::new(recorded) as RetryAttemptValue)
        })
    }
}
