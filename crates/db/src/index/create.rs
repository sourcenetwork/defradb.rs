//! Index creation: the definition and its `BACKFILL_INDEX` action commit
//! under the collection's write guard; the documents that already exist are
//! indexed afterwards by the batches in [`backfill`](crate::index::backfill).

use std::sync::Arc;

use futures::channel::oneshot;
use schema::{IndexDescription, IndexKind, IndexedFieldDescription, OrderedIndexDescription};
use storage::corekv::{Key, Store};
use storage::keys::systemstore::{CollectionKey, CollectionNameKey};

use crate::database::action::ActionExecutionLease;
use crate::database::spawn::spawn_task;
use crate::error::{Error, Result};
use crate::index::backfill::{encode_progress, BackfillPlan};
use crate::index::IndexManager;
use crate::DB;

enum IndexDefinition {
    Ready(IndexDescription),
    Backfill(IndexDescription, ActionExecutionLease, BackfillPlan),
}

impl<S: Store> DB<S> {
    /// Create an index of `kind` over `fields` on `collection_name`, then
    /// index every document the collection already holds. A missing or empty
    /// `name` is generated.
    ///
    /// The definition, the in-progress action and the backfill's fence (the
    /// first short id no existing document has) commit under the write guard
    /// a truncate or a patch holds, and the cache reloads before that guard
    /// is released, so every write landing afterwards maintains the index
    /// itself. The backfill of the documents below the fence then runs in
    /// bounded transactions that hold no guard, detached from this future so
    /// a caller that gives up on waiting does not stop it; the planner uses
    /// the index once the action completes. A failed backfill leaves the
    /// definition in place and the failure on the action.
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
        let definition = self
            .define_index(collection_name, name, fields, kind, false)
            .await?;
        self.finish_index_definition(definition).await
    }

    /// Reconcile a named nonunique ordered index without changing collection
    /// identity. Concurrent calls either observe the completed index or report
    /// its existing backfill; they never start another backfill or claim that
    /// an unfinished or failed index is ready.
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
        let definition = self
            .define_index(
                collection_name,
                Some(name),
                fields,
                IndexKind::Ordered(OrderedIndexDescription { unique: false }),
                true,
            )
            .await?;
        self.finish_index_definition(definition).await
    }

    async fn finish_index_definition(
        self: &Arc<Self>,
        definition: IndexDefinition,
    ) -> Result<IndexDescription>
    where
        S: 'static,
    {
        let (index, lease, plan) = match definition {
            IndexDefinition::Ready(index) => return Ok(index),
            IndexDefinition::Backfill(index, lease, plan) => (index, lease, plan),
        };
        let (done, outcome) = oneshot::channel();
        let db = Arc::clone(self);
        spawn_task(async move {
            let _ = done.send(db.backfill_index(lease, plan).await);
        });
        outcome
            .await
            .map_err(|_| Error::Other("the index backfill ended without reporting".into()))??;
        Ok(index)
    }

    async fn define_index(
        &self,
        collection_name: &str,
        name: Option<&str>,
        fields: Vec<IndexedFieldDescription>,
        kind: IndexKind,
        reconcile: bool,
    ) -> Result<IndexDefinition> {
        let collection_id = self
            .require_collection(collection_name)?
            .collection_id()
            .to_string();
        let _guards = self
            .collection_write_guards(std::iter::once(collection_id.clone()))
            .await?;
        let collection = self.require_collection(collection_name)?;
        if reconcile {
            if let Some(index) = collection
                .schema()
                .indexes
                .iter()
                .find(|index| Some(index.name.as_str()) == name)
            {
                if index.fields != fields || index.clone().normalized().kind != Some(kind) {
                    return Err(Error::Other(format!(
                        "index '{}' on '{collection_name}' has a different definition",
                        index.name
                    )));
                }
                if let Some(action) = self
                    .list_index_actions(&collection_id)
                    .await?
                    .get(&index.id)
                {
                    if action.status == defra_core::ActionStatus::IN_PROGRESS {
                        return Err(Error::ActionInProgress {
                            collection_id,
                            action: defra_core::Action::BACKFILL_INDEX.value(),
                        });
                    }
                    return Err(Error::Other(format!(
                        "index '{}' on '{collection_name}' is not ready: {}",
                        index.name, action.reason
                    )));
                }
                return Ok(IndexDefinition::Ready(index.clone()));
            }
        }
        let fence = self.peek_doc_short_id().await?;

        let txn = self.new_txn(false).await?;
        let (index, lease) = {
            let datastore = txn.datastore()?;
            let systemstore = txn.systemstore()?;
            let mut manager = IndexManager::from_collection(
                collection.schema().resolved_root_id(),
                collection.schema(),
            )?;
            let index = manager
                .create_index_of_kind(
                    &datastore,
                    collection_name,
                    name.unwrap_or("").to_string(),
                    fields,
                    kind,
                    &collection.schema().fields,
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
            let lease = self
                .stage_action(
                    &systemstore,
                    &collection_id,
                    defra_core::Action::BACKFILL_INDEX,
                    &index.id.to_string(),
                )
                .await?;
            let plan = BackfillPlan::new(collection_name, &collection_id, &index, fence, 0);
            systemstore
                .set(&plan.progress_key().bytes(), &encode_progress(fence, 0))
                .await?;
            (index, lease)
        };
        txn.commit().await?;
        self.publish_started_action(&lease);
        self.reload_cache().await?;
        let plan = BackfillPlan::new(collection_name, &collection_id, &index, fence, 0);
        Ok(IndexDefinition::Backfill(index, lease, plan))
    }
}
