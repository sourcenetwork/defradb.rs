//! The batched index backfill and its recovery after a restart.
//!
//! Documents below the fence are walked in short id order, a bounded number
//! per transaction, each one observed as well as scanned so a batch that
//! indexed a value the document no longer has aborts and runs again. A
//! document stored at an older schema version with a transform on its path
//! is migrated and persisted, with every index, before it is indexed, the
//! way a read would materialize it. The last short id a batch indexed
//! commits with it under the action's progress key, so a restart resumes
//! after the last durable batch. No guard is held: writers keep the index
//! current for their own documents from the moment the definition commits.

use rapidhash::RapidHashMap;
use std::sync::Arc;

use async_trait::async_trait;
use datastore::NamespaceView;
use defra_core::{Action, ActionStatus};
use document::Document;
use lens::{TargetedHistoryLink, TransformStore};
use schema::IndexDescription;
use storage::corekv::{Key, Store};
use storage::keys::systemstore::ActionProgressKey;

use crate::collection::Collection;
use crate::database::action::{action_executions, stage_action_failure, ActionExecutionLease};
use crate::definition::lens::build_collection_history;
use crate::definition::loader::get_collections_by_collection_id;
use crate::definition::migration::helpers::{
    cache_migrated_document_with_indexes, migrate_to_active, ToActive,
};
use crate::error::{Error, Result};
use crate::index::manager::DocumentSource;
use crate::index::pages::{Page, PAGE_ITEMS};
use crate::index::{BatchIndexResult, IndexManager};
use crate::{BackfillSource, DB};

type History = RapidHashMap<String, TargetedHistoryLink>;

/// Documents per backfill transaction at most (`PAGE_ITEMS`).
pub const BACKFILL_BATCH_DOCS: usize = PAGE_ITEMS;

/// What a backfill needs to know to run or resume.
#[derive(Debug, Clone)]
pub struct BackfillPlan {
    collection_name: String,
    collection_id: String,
    index_id: u32,
    index_name: String,
    fence: u64,
    last: u64,
}

impl BackfillPlan {
    pub(crate) fn new(
        collection_name: &str,
        collection_id: &str,
        index: &IndexDescription,
        fence: u64,
        last: u64,
    ) -> Self {
        Self {
            collection_name: collection_name.to_string(),
            collection_id: collection_id.to_string(),
            index_id: index.id,
            index_name: index.name.clone(),
            fence,
            last,
        }
    }

    pub(crate) fn progress_key(&self) -> ActionProgressKey {
        ActionProgressKey::new(
            &self.collection_id,
            Action::BACKFILL_INDEX,
            self.index_id.to_string(),
        )
    }
}

/// The progress record: the fence, then the last short id indexed (0 before
/// the first batch commits).
pub fn encode_progress(fence: u64, last: u64) -> [u8; 16] {
    let mut value = [0u8; 16];
    value[..8].copy_from_slice(&fence.to_be_bytes());
    value[8..].copy_from_slice(&last.to_be_bytes());
    value
}

/// The inverse of [`encode_progress`]; `None` for a record of another shape.
pub fn decode_progress(bytes: &[u8]) -> Option<(u64, u64)> {
    let fence = bytes.get(..8)?.try_into().ok()?;
    let last = bytes.get(8..)?.try_into().ok()?;
    Some((u64::from_be_bytes(fence), u64::from_be_bytes(last)))
}

impl<S: Store> DB<S> {
    /// Run `plan` to the end and settle its action either way.
    pub(crate) async fn backfill_index(
        &self,
        lease: ActionExecutionLease,
        plan: BackfillPlan,
    ) -> Result<()> {
        match self.backfill_batches(&plan).await {
            Ok(indexed) => {
                tracing::info!(
                    collection_id = %plan.collection_id,
                    index_id = plan.index_id,
                    indexed,
                    "index backfill completed"
                );
                self.complete_action(lease).await?;
            }
            Err(error) => {
                let reason = error.to_string();
                if self.record_backfill_failure(&plan, &reason).await? {
                    tracing::error!(
                        collection_id = %plan.collection_id,
                        index_id = plan.index_id,
                        %error,
                        "index backfill failed"
                    );
                    self.publish_failed_action(&lease, &reason);
                } else {
                    tracing::info!(
                        collection_id = %plan.collection_id,
                        index_id = plan.index_id,
                        "index dropped during its backfill"
                    );
                }
            }
        }
        self.reload_cache().await
    }

    /// Write a backfill's failure on its record, unless its index is gone.
    /// The drop deletes the record in the commit that removes the index, so
    /// a failure written after it would show a failed backfill for an index
    /// that no longer exists. The definition is read in the same transaction
    /// as the write, so a drop committing first makes the attempt run again.
    async fn record_backfill_failure(&self, plan: &BackfillPlan, reason: &str) -> Result<bool> {
        let attempt = FailBackfillAttempt {
            plan: plan.clone(),
            reason: reason.to_string(),
        };
        match self
            .store()
            .transact_retry(self.options().retry_options(), Box::new(attempt))
            .await
        {
            Ok(value) => value
                .downcast::<bool>()
                .map(|recorded| *recorded)
                .map_err(|_| Error::Other("invalid backfill failure result".into())),
            Err(error) => Err(match error.downcast_abort::<Error>() {
                Ok(error) => error,
                Err(error) => Error::Storage(error.into_storage()),
            }),
        }
    }

    /// Resume every backfill a restart interrupted, before the database
    /// takes any write; nothing runs alongside, so open waits for them.
    pub(crate) async fn resume_index_backfills(&self) -> Result<()> {
        let txn = self.new_txn(true).await?;
        let mut pending = Vec::new();
        {
            let systemstore = txn.systemstore()?;
            for execution in action_executions(&systemstore).await? {
                if execution.action != Action::BACKFILL_INDEX {
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
                    .and_then(|value| decode_progress(&value));
                pending.push((execution, progress));
            }
        }
        txn.discard()?;

        for (execution, progress) in pending {
            let index = execution.subject.parse::<u32>().ok().and_then(|index_id| {
                self.find_collection_by_id(&execution.collection_id)
                    .ok()
                    .flatten()
                    .and_then(|collection| {
                        let index = collection
                            .schema()
                            .indexes
                            .iter()
                            .find(|index| index.id == index_id)
                            .cloned()?;
                        Some((collection.name().to_string(), index))
                    })
            });
            let Some((collection_name, index)) = index else {
                tracing::warn!(
                    collection_id = %execution.collection_id,
                    index = %execution.subject,
                    "clearing an index backfill whose index no longer exists"
                );
                self.clear_action(
                    &execution.collection_id,
                    execution.action,
                    &execution.subject,
                )
                .await?;
                continue;
            };
            if execution.status != ActionStatus::IN_PROGRESS {
                continue;
            }
            let (fence, last) = match progress {
                Some(progress) => progress,
                None => (self.reserve_doc_short_id_fence().await?, 0),
            };
            let lease = self.resume_action(
                &execution.collection_id,
                execution.action,
                &execution.subject,
            )?;
            tracing::info!(
                collection_id = %execution.collection_id,
                index_id = index.id,
                last,
                "resuming index backfill"
            );
            let plan = BackfillPlan::new(
                &collection_name,
                &execution.collection_id,
                &index,
                fence,
                last,
            );
            self.backfill_index(lease, plan).await?;
        }
        Ok(())
    }

    async fn backfill_batches(&self, plan: &BackfillPlan) -> Result<usize> {
        let mut last = plan.last;
        let mut indexed = 0;
        let mut max_docs = BACKFILL_BATCH_DOCS;
        loop {
            let batch = self.backfill_batch(plan, last, &mut max_docs).await?;
            indexed += batch.indexed;
            if let Some(id) = batch.last_doc_short_id {
                last = id;
            }
            if batch.exhausted {
                break;
            }
        }

        let attempt = FinishIndexAttempt {
            collection: self.require_collection(&plan.collection_name)?,
            index_name: plan.index_name.clone(),
        };
        match self
            .store()
            .transact_retry(self.options().retry_options(), Box::new(attempt))
            .await
        {
            Ok(_) => {}
            Err(error) => {
                return Err(match error.downcast_abort::<Error>() {
                    Ok(error) => error,
                    Err(error) => Error::Storage(error.into_storage()),
                })
            }
        }
        Ok(indexed)
    }

    async fn backfill_batch(
        &self,
        plan: &BackfillPlan,
        last: u64,
        max_docs: &mut usize,
    ) -> Result<BatchIndexResult> {
        let page = BackfillPage {
            collection: self.require_collection(&plan.collection_name)?,
            plan: plan.clone(),
            last,
            lens_store: Arc::clone(&self.lens_store),
        };
        self.run_page(page, max_docs).await
    }
}

/// Publish the completed index against the same definition snapshot as its build.
struct FinishIndexAttempt {
    collection: Collection,
    index_name: String,
}

impl storage::corekv::RetryAttemptFn for FinishIndexAttempt {
    fn run<'a>(
        &'a mut self,
        retry: &'a mut dyn storage::corekv::RetryTxn,
    ) -> storage::corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            let io = retry.begin_attempt_io()?;
            let txn = datastore::BasicTxn::from_retry_attempt_io(io, 0);
            let datastore = txn.datastore();
            let systemstore = txn.systemstore();
            let built: Result<()> = async move {
                let collection = crate::collection::loader::load_collection_for_write_attempt(
                    &systemstore,
                    self.collection.name(),
                    &self.collection,
                )
                .await?;
                ensure_backfill_index_unchanged(&self.collection, &collection, &self.index_name)?;
                let manager = IndexManager::from_collection(
                    collection.schema().resolved_root_id(),
                    collection.schema(),
                )?;
                manager.build_index(&datastore, &self.index_name).await?;
                Ok(())
            }
            .await;
            built.map_err(|error| storage::corekv::RetryAttemptError::Abort(Box::new(error)))?;
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(Box::new(()) as storage::corekv::RetryAttemptValue)
        })
    }
}

/// Settle a failed backfill against the definition the attempt reads.
struct FailBackfillAttempt {
    plan: BackfillPlan,
    reason: String,
}

impl storage::corekv::RetryAttemptFn for FailBackfillAttempt {
    fn run<'a>(
        &'a mut self,
        retry: &'a mut dyn storage::corekv::RetryTxn,
    ) -> storage::corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            let io = retry.begin_attempt_io()?;
            let txn = datastore::BasicTxn::from_retry_attempt_io(io, 0);
            let systemstore = txn.systemstore();
            let recorded: Result<bool> = async {
                let collection = crate::collection::loader::load_collection_from_systemstore(
                    &systemstore,
                    &self.plan.collection_name,
                )
                .await
                .map_err(|error| Error::Other(error.to_string()))?;
                let defined = collection.is_some_and(|collection| {
                    collection
                        .schema()
                        .indexes
                        .iter()
                        .any(|index| index.id == self.plan.index_id)
                });
                if defined {
                    stage_action_failure(
                        &systemstore,
                        &self.plan.collection_id,
                        Action::BACKFILL_INDEX,
                        &self.plan.index_id.to_string(),
                        &self.reason,
                    )
                    .await?;
                    systemstore
                        .delete(&self.plan.progress_key().bytes())
                        .await
                        .map_err(Error::Storage)?;
                }
                Ok(defined)
            }
            .await;
            let recorded = recorded
                .map_err(|error| storage::corekv::RetryAttemptError::Abort(Box::new(error)))?;
            drop(systemstore);
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(Box::new(recorded) as storage::corekv::RetryAttemptValue)
        })
    }
}

/// One backfill batch: the documents after `last` and below the fence.
struct BackfillPage {
    collection: Collection,
    plan: BackfillPlan,
    last: u64,
    lens_store: Arc<dyn TransformStore>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Page for BackfillPage {
    type Output = BatchIndexResult;

    async fn run(
        &mut self,
        txn: &mut datastore::BasicTxn,
        size: usize,
    ) -> Result<BatchIndexResult> {
        self.index(txn.datastore(), txn.systemstore(), size).await
    }
}

impl BackfillPage {
    async fn index(
        &self,
        datastore: NamespaceView,
        systemstore: NamespaceView,
        max_docs: usize,
    ) -> Result<BatchIndexResult> {
        let collection = crate::collection::loader::load_collection_for_write_attempt(
            &systemstore,
            self.collection.name(),
            &self.collection,
        )
        .await?;
        ensure_backfill_index_unchanged(&self.collection, &collection, &self.plan.index_name)?;
        let versions =
            get_collections_by_collection_id(&systemstore, &self.plan.collection_id).await?;
        let history = build_collection_history(&versions, collection.version_id())
            .ok_or_else(|| Error::Lens("failed to build backfill migration history".into()))?;
        let mut source = MigratingSource {
            inner: BackfillSource::open_range(
                collection.clone(),
                datastore.clone(),
                systemstore.clone(),
                (self.last > 0).then_some(self.last),
                Some(self.plan.fence),
            )
            .await?,
            collection: collection.clone(),
            datastore: datastore.clone(),
            systemstore: systemstore.clone(),
            lens_store: Arc::clone(&self.lens_store),
            history,
        };
        let batch = index_batch(
            &collection,
            &datastore,
            &systemstore,
            &self.plan,
            &mut source,
            max_docs,
        )
        .await?;
        Ok(batch)
    }
}

fn ensure_backfill_index_unchanged(
    expected: &Collection,
    current: &Collection,
    index_name: &str,
) -> Result<()> {
    let expected_index = expected
        .get_index(index_name)
        .ok_or_else(|| Error::Other(format!("index '{index_name}' disappeared during backfill")))?;
    let current_index = current
        .get_index(index_name)
        .ok_or_else(|| Error::Other(format!("index '{index_name}' changed during backfill")))?;
    if expected_index != current_index {
        return Err(Error::Other(format!(
            "index '{index_name}' changed during backfill"
        )));
    }
    Ok(())
}

/// The raw documents below the fence, each one at an older schema version
/// with a transform on its path migrated and persisted first.
struct MigratingSource {
    inner: BackfillSource,
    collection: Collection,
    datastore: NamespaceView,
    systemstore: NamespaceView,
    lens_store: Arc<dyn TransformStore>,
    history: History,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocumentSource for MigratingSource {
    async fn next(&mut self) -> crate::index::error::Result<Option<(u64, Document)>> {
        use crate::index::error::Error as IndexError;

        let Some((doc_short_id, doc)) = self.inner.next().await? else {
            return Ok(None);
        };
        let ToActive::Migrated(migrated) =
            migrate_to_active(&doc, &self.collection, &self.history, &self.lens_store)
                .await
                .map_err(|error| IndexError::Other(error.to_string()))?
        else {
            return Ok(Some((doc_short_id, doc)));
        };
        cache_migrated_document_with_indexes(
            &self.datastore,
            &self.systemstore,
            &self.collection,
            &migrated,
        )
        .await
        .map_err(|e| IndexError::Other(e.to_string()))?;
        Ok(Some((doc_short_id, migrated)))
    }
}

async fn index_batch(
    collection: &Collection,
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    plan: &BackfillPlan,
    source: &mut MigratingSource,
    max_docs: usize,
) -> Result<BatchIndexResult> {
    let manager =
        IndexManager::from_collection(collection.schema().resolved_root_id(), collection.schema())?;
    let batch = manager
        .index_batch_from(
            datastore,
            &plan.index_name,
            source,
            collection.schema(),
            max_docs,
        )
        .await?;
    if let Some(id) = batch.last_doc_short_id {
        systemstore
            .set(
                &plan.progress_key().bytes(),
                &encode_progress(plan.fence, id),
            )
            .await
            .map_err(Error::Storage)?;
    }
    Ok(batch)
}
