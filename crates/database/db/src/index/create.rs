//! Index creation commits a schema-version dependency and its `BACKFILL_INDEX`
//! action; the documents that already exist are indexed afterwards by the
//! batches in [`backfill`](crate::index::backfill).

use std::sync::Arc;

use futures::channel::oneshot;
use schema::{IndexDescription, IndexKind, IndexedFieldDescription, OrderedIndexDescription};
use storage::corekv::{
    Key, OnCommit, RetryAttemptError, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue,
    RetryTxn, Store,
};
use storage::keys::systemstore::{CollectionKey, CollectionNameKey};

use crate::collection::Collection;
use crate::database::action::{stage_action_start, ActionExecutionLease};
use crate::database::spawn::spawn_task;
use crate::error::{Error, Result};
use crate::index::backfill::BackfillPlan;
use crate::index::IndexManager;
use crate::txn::DbTxn;
use crate::DB;

/// What reconciling a named index found: the index ready, or a definition
/// this call committed whose backfill is still to run.
enum IndexDefinition {
    Ready(IndexDescription),
    Defined(IndexDescription),
}

/// The index definition an attempt stages inside the definition transaction.
pub(super) struct IndexCreation<'name> {
    pub(super) name: Option<&'name str>,
    pub(super) fields: Vec<IndexedFieldDescription>,
    pub(super) kind: IndexKind,
}

/// One attempt of [`DB::define_index`]'s definition change.
struct DefineIndexAttempt<S>
where
    S: Store + 'static,
{
    db: Arc<DB<S>>,
    collection: Collection,
    collection_name: String,
    name: Option<String>,
    fields: Vec<IndexedFieldDescription>,
    kind: IndexKind,
}

impl<S> RetryAttemptFn for DefineIndexAttempt<S>
where
    S: Store + 'static,
{
    fn run<'a>(&'a mut self, retry: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let io = retry.begin_attempt_io()?;
            let txn = self
                .db
                .new_db_txn(datastore::BasicTxn::from_retry_attempt_io(
                    io,
                    0,
                    self.db.crdts().clone(),
                ));
            let collection = txn
                .load_collection_for_write_attempt(&self.collection_name, &self.collection)
                .await
                .map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
            txn.validate_collection_for_write(&collection)
                .await
                .map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
            let index = self
                .db
                .stage_index_definition(
                    &txn,
                    &collection,
                    &self.collection_name,
                    IndexCreation {
                        name: self.name.as_deref(),
                        fields: self.fields.clone(),
                        kind: self.kind,
                    },
                )
                .await
                .map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
            let collection_id = collection.collection_id().to_string();
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(Box::new((index, collection_id)) as RetryAttemptValue)
        })
    }
}

impl<S: Store> DB<S> {
    /// Create an index of `kind` over `fields` on `collection_name`, then
    /// index every document the collection already holds. A missing or empty
    /// `name` is generated.
    ///
    /// The definition and in-progress action commit together, and the
    /// commit publishes them to the collection cache before it returns,
    /// before the new fence is captured. This makes writes that land
    /// before publication part of the backfill range and lets later writes
    /// maintain the index themselves. Backfill then runs in bounded
    /// transactions detached from this future so a caller that gives up on
    /// waiting does not stop it; the planner uses the index once the action
    /// completes. A failed backfill leaves the definition in place and the
    /// failure on the action.
    pub async fn create_index(
        self: &Arc<Self>,
        collection_name: &str,
        name: Option<&str>,
        fields: Vec<IndexedFieldDescription>,
        kind: IndexKind,
    ) -> Result<IndexDescription>
    where
        S: 'static,
    {
        let (index, lease, plan) = self
            .define_index(collection_name, name, fields, kind)
            .await?;
        self.backfill_defined_index(index, lease, plan).await
    }

    /// [`Self::create_index`], returning once the definition has committed:
    /// the index is listed as building until its backfill, a job of its
    /// own, completes or fails. Go's `Collection.NewIndex`, which never waits
    /// for the build.
    pub async fn start_index(
        self: &Arc<Self>,
        collection_name: &str,
        name: Option<&str>,
        fields: Vec<IndexedFieldDescription>,
        kind: IndexKind,
    ) -> Result<IndexDescription>
    where
        S: 'static,
    {
        let (index, lease, plan) = self
            .define_index(collection_name, name, fields, kind)
            .await?;
        let db = Arc::clone(self);
        let collection = collection_name.to_owned();
        let index_id = index.id;
        spawn_task(self.pool(), "db::index_backfill", async move {
            // The build's outcome is on the index's action record. An error
            // here is settling that record, and nothing else reports it.
            if let Err(error) = db.backfill_index(lease, plan).await {
                tracing::error!(
                    collection = %collection,
                    index_id,
                    %error,
                    "index backfill could not be settled"
                );
            }
        });
        Ok(index)
    }

    /// Reconcile a named nonunique ordered index without changing collection
    /// identity. Concurrent calls either observe the completed index or report
    /// its existing backfill; they never start another backfill or claim that
    /// an unfinished or failed index is ready.
    ///
    /// The decision reads the definition and the index's action in the
    /// transaction that would define the index, so a concurrent definition
    /// committing first makes it run again against the new definition.
    pub async fn ensure_ordinary_index(
        self: &Arc<Self>,
        collection_name: &str,
        name: &str,
        fields: Vec<IndexedFieldDescription>,
    ) -> Result<IndexDescription>
    where
        S: 'static,
    {
        if name.is_empty() {
            return Err(Error::Other(
                "index reconciliation requires a stable name".into(),
            ));
        }
        let collection = self.require_collection(collection_name)?;
        let collection_id = collection.collection_id().to_string();
        let attempt = EnsureIndexAttempt {
            db: Arc::clone(self),
            collection_name: collection_name.to_string(),
            collection,
            name: name.to_string(),
            fields,
        };
        let definition = match self
            .store()
            .transact_retry(self.options().retry_options(), Box::new(attempt))
            .await
        {
            Ok(value) => *value
                .downcast::<IndexDefinition>()
                .map_err(|_| Error::Other("invalid index reconciliation result".into()))?,
            Err(error) => {
                return Err(match error.downcast_abort::<Error>() {
                    Ok(error) => error,
                    Err(error) => Error::Storage(error.into_storage()),
                })
            }
        };
        let index = match definition {
            IndexDefinition::Ready(index) => return Ok(index),
            IndexDefinition::Defined(index) => index,
        };
        let (lease, plan) = self
            .publish_index_definition(collection_name, &collection_id, &index)
            .await?;
        self.backfill_defined_index(index, lease, plan).await
    }

    /// Run the backfill of a committed definition as a job of its own, so it
    /// finishes even when the caller stops waiting, and report its outcome.
    async fn backfill_defined_index(
        self: &Arc<Self>,
        index: IndexDescription,
        lease: ActionExecutionLease,
        plan: BackfillPlan,
    ) -> Result<IndexDescription>
    where
        S: 'static,
    {
        let (done, outcome) = oneshot::channel();
        let db = Arc::clone(self);
        spawn_task(self.pool(), "db::index_backfill", async move {
            let _ = done.send(db.backfill_index(lease, plan).await);
        });
        outcome
            .await
            .map_err(|_| Error::Other("the index backfill ended without reporting".into()))??;
        Ok(index)
    }

    async fn define_index(
        self: &Arc<Self>,
        collection_name: &str,
        name: Option<&str>,
        fields: Vec<IndexedFieldDescription>,
        kind: IndexKind,
    ) -> Result<(IndexDescription, ActionExecutionLease, BackfillPlan)>
    where
        S: 'static,
    {
        let collection = self.require_collection(collection_name)?;
        let attempt = DefineIndexAttempt {
            db: Arc::clone(self),
            collection,
            collection_name: collection_name.to_string(),
            name: name.map(str::to_owned),
            fields,
            kind,
        };
        let (index, collection_id) = match self
            .store()
            .transact_retry(self.options().retry_options(), Box::new(attempt))
            .await
        {
            Ok(value) => value
                .downcast::<(IndexDescription, String)>()
                .map(|defined| *defined)
                .map_err(|_| Error::Other("invalid index definition result".into()))?,
            Err(error) => {
                return Err(match error.downcast_abort::<Error>() {
                    Ok(error) => error,
                    Err(error) => Error::Storage(error.into_storage()),
                })
            }
        };
        let (lease, plan) = self
            .publish_index_definition(collection_name, &collection_id, &index)
            .await?;
        Ok((index, lease, plan))
    }

    /// Write the definition of a new index and its in-progress action into
    /// `txn`, and publish them to the collection cache when `txn` commits,
    /// before the commit returns: the caller's next request, the backfill
    /// first of all, works from this definition, even while writers' commit
    /// hooks hold back ordered events.
    pub(super) async fn stage_index_definition(
        &self,
        txn: &DbTxn<S>,
        collection: &Collection,
        collection_name: &str,
        creation: IndexCreation<'_>,
    ) -> Result<IndexDescription> {
        crate::chunk::ensure_admin_changeable(collection.schema())?;
        let collection_id = collection.collection_id().to_string();
        let (index, committed_schema) = {
            let datastore = txn.datastore()?;
            let systemstore = txn.systemstore()?;
            let mut manager = IndexManager::from_collection(
                collection.schema().resolved_root_id(),
                collection.schema(),
                collection.crdts(),
            )?;
            let index = manager
                .create_index_of_kind(
                    &datastore,
                    collection_name,
                    creation.name.unwrap_or("").to_string(),
                    creation.fields,
                    creation.kind,
                )
                .await?;
            let mut schema = collection.schema().clone();
            schema.indexes.push(index.clone());
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
            stage_action_start(
                &systemstore,
                &collection_id,
                defra_core::Action::BACKFILL_INDEX,
                &index.id.to_string(),
            )
            .await?;
            (index, schema)
        };
        let cached_collection = crate::collection::Collection::load_index_actions(
            committed_schema,
            &txn.systemstore()?,
        )
        .await?;
        let cache = std::sync::Arc::clone(&self.collections);
        let index_id = index.id;
        txn.on_commit(OnCommit::OrderedAndWait(Box::new(move || {
            cache.publish(
                cached_collection,
                &[(index_id, Some(defra_core::ActionStatus::IN_PROGRESS))],
            );
        })))?;
        Ok(index)
    }

    /// Claim and announce the backfill of a committed definition.
    pub(super) async fn publish_index_definition(
        &self,
        collection_name: &str,
        collection_id: &str,
        index: &IndexDescription,
    ) -> Result<(ActionExecutionLease, BackfillPlan)> {
        // A competing definition attempt may choose the same ID until commit
        // validation picks its winner. Only committed definitions claim a job.
        let lease = self.resume_action(
            collection_id,
            defra_core::Action::BACKFILL_INDEX,
            &index.id.to_string(),
        )?;
        // The fence is a short-ID cutoff taken after the definition commits.
        // It is sound without the definition commit's sequence because every
        // writer reads the definition key inside its own transaction:
        // - a writer that commits before the definition allocated its short ID
        //   before it committed, so the ID is below this cutoff and a backfill
        //   batch, which reads after the definition, indexes the document;
        // - a writer whose snapshot predates the definition but commits after
        //   it read the old definition key, a validated read, so it conflicts
        //   and its re-run sees the index, whatever its short ID;
        // - a writer whose snapshot follows the definition maintains the index
        //   itself, and a backfill batch that read the document first conflicts
        //   on its validated read rather than writing a stale entry.
        // tests/index/fence.rs and tests/index/create.rs drive each case. If the
        // process stops before the first batch persists progress, startup
        // captures a fresh fence after publication.
        let fence = self.reserve_doc_short_id_fence().await?;
        self.publish_started_action(&lease);
        Ok((
            lease,
            BackfillPlan::new(collection_name, collection_id, index, fence, 0),
        ))
    }

    /// One attempt of [`ensure_ordinary_index`](Self::ensure_ordinary_index):
    /// the named index as the definition this attempt reads has it, or its
    /// definition staged in `txn` when it has none.
    async fn reconcile_ordinary_index(
        &self,
        txn: &DbTxn<S>,
        expected: &Collection,
        collection_name: &str,
        name: &str,
        fields: Vec<IndexedFieldDescription>,
    ) -> Result<IndexDefinition> {
        let kind = IndexKind::Ordered(OrderedIndexDescription { unique: false });
        let collection = txn
            .load_collection_for_write_attempt(collection_name, expected)
            .await?;
        let Some(index) = collection
            .schema()
            .indexes
            .iter()
            .find(|index| index.name == name)
        else {
            let index = self
                .stage_index_definition(
                    txn,
                    &collection,
                    collection_name,
                    IndexCreation {
                        name: Some(name),
                        fields,
                        kind,
                    },
                )
                .await?;
            return Ok(IndexDefinition::Defined(index));
        };
        if index.fields != fields || index.clone().normalized().kind != Some(kind) {
            return Err(Error::Other(format!(
                "index '{}' on '{collection_name}' has a different definition",
                index.name
            )));
        }
        let collection_id = collection.collection_id();
        let actions =
            crate::database::action::index_action_executions(&txn.systemstore()?, collection_id)
                .await?;
        if let Some(action) = actions.get(&index.id) {
            if action.status == defra_core::ActionStatus::IN_PROGRESS {
                return Err(Error::ActionInProgress {
                    collection_id: collection_id.to_string(),
                    action: defra_core::Action::BACKFILL_INDEX.value(),
                });
            }
            return Err(Error::Other(format!(
                "index '{}' on '{collection_name}' is not ready: {}",
                index.name, action.reason
            )));
        }
        Ok(IndexDefinition::Ready(index.clone()))
    }
}

struct EnsureIndexAttempt<S>
where
    S: Store + 'static,
{
    db: Arc<DB<S>>,
    collection_name: String,
    collection: Collection,
    name: String,
    fields: Vec<IndexedFieldDescription>,
}

impl<S> RetryAttemptFn for EnsureIndexAttempt<S>
where
    S: Store + 'static,
{
    fn run<'a>(&'a mut self, retry: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let io = retry.begin_attempt_io()?;
            let txn = self
                .db
                .new_db_txn(datastore::BasicTxn::from_retry_attempt_io(
                    io,
                    0,
                    self.db.crdts().clone(),
                ));
            let definition = self
                .db
                .reconcile_ordinary_index(
                    &txn,
                    &self.collection,
                    &self.collection_name,
                    &self.name,
                    self.fields.clone(),
                )
                .await
                .map_err(|error| RetryAttemptError::Abort(Box::new(error)))?;
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(Box::new(definition) as RetryAttemptValue)
        })
    }
}
