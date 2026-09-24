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
use lens::{Lens, TargetedHistoryLink, TransformStore};
use schema::IndexDescription;
use storage::corekv::{Key, Store};
use storage::keys::systemstore::ActionProgressKey;

use crate::collection::Collection;
use crate::database::action::{action_executions, ActionExecutionLease};
use crate::definition::lens::{
    build_collection_history, doc_to_lens_doc, migration_path_has_transform,
};
use crate::definition::loader::get_collections_by_collection_id;
use crate::definition::migration::helpers::{
    cache_migrated_document_with_indexes, lens_doc_to_document,
};
use crate::error::{Error, Result};
use crate::index::manager::DocumentSource;
use crate::index::{BatchIndexResult, IndexManager};
use crate::{BackfillSource, DB};

type History = RapidHashMap<String, TargetedHistoryLink>;

/// Documents per backfill transaction at most. Each adds a handful of index
/// keys to the write set, so a batch, and with it the window in which a
/// concurrent write to one of its documents conflicts, stays well under a
/// millisecond for an ordered index. A conflict halves the next attempt
/// down to one document, so a stream of writes into the range being walked
/// slows the backfill rather than starving it; a commit doubles it back.
pub const BACKFILL_BATCH_DOCS: usize = 256;

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
                tracing::error!(
                    collection_id = %plan.collection_id,
                    index_id = plan.index_id,
                    %error,
                    "index backfill failed"
                );
                self.fail_action(lease, &error.to_string()).await?;
            }
        }
        self.reload_cache().await
    }

    /// Resume every backfill a restart interrupted, before the database
    /// takes any write; nothing runs alongside, so open waits for them.
    pub(crate) async fn resume_index_backfills(&self) -> Result<()> {
        let txn = self.new_txn(true).await?;
        let mut pending = Vec::new();
        {
            let systemstore = txn.systemstore()?;
            for execution in action_executions(&systemstore).await? {
                if execution.action != Action::BACKFILL_INDEX
                    || execution.status != ActionStatus::IN_PROGRESS
                {
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
            let (fence, last) = match progress {
                Some(progress) => progress,
                None => (self.peek_doc_short_id().await?, 0),
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
        let history = self.migration_history(plan).await?;
        let mut last = plan.last;
        let mut indexed = 0;
        let mut max_docs = BACKFILL_BATCH_DOCS;
        loop {
            let batch = self
                .backfill_batch(plan, &history, last, &mut max_docs)
                .await?;
            indexed += batch.indexed;
            if let Some(id) = batch.last_doc_short_id {
                last = id;
            }
            if batch.exhausted {
                break;
            }
        }

        let collection = self.require_collection(&plan.collection_name)?;
        let txn = self.new_txn(false).await?;
        let built = {
            let datastore = txn.datastore()?;
            match IndexManager::from_collection(
                collection.schema().resolved_root_id(),
                collection.schema(),
            ) {
                Ok(manager) => manager
                    .build_index(&datastore, &plan.index_name)
                    .await
                    .map_err(Error::from),
                Err(error) => Err(error.into()),
            }
        };
        if let Err(error) = built {
            txn.discard()?;
            return Err(error);
        }
        txn.commit().await?;
        Ok(indexed)
    }

    async fn migration_history(&self, plan: &BackfillPlan) -> Result<History> {
        let collection = self.require_collection(&plan.collection_name)?;
        let txn = self.new_txn(true).await?;
        let versions = {
            let systemstore = txn.systemstore()?;
            get_collections_by_collection_id(&systemstore, &plan.collection_id).await?
        };
        txn.discard()?;
        build_collection_history(&versions, collection.version_id()).ok_or_else(|| {
            Error::Lens(format!(
                "failed to build migration history for collection '{}'",
                plan.collection_name
            ))
        })
    }

    async fn backfill_batch(
        &self,
        plan: &BackfillPlan,
        history: &History,
        last: u64,
        max_docs: &mut usize,
    ) -> Result<BatchIndexResult> {
        let max_retries = self.options().max_txn_retries();
        let mut retries_at_one = 0;
        loop {
            let collection = self.require_collection(&plan.collection_name)?;
            let txn = self.new_txn(false).await?;
            let outcome = {
                let datastore = txn.datastore()?;
                let systemstore = txn.systemstore()?;
                let mut source = MigratingSource {
                    inner: BackfillSource::open_range(
                        collection.clone(),
                        datastore.clone(),
                        systemstore.clone(),
                        (last > 0).then_some(last),
                        Some(plan.fence),
                    )
                    .await?,
                    collection: collection.clone(),
                    datastore: datastore.clone(),
                    systemstore: systemstore.clone(),
                    lens_store: Arc::clone(&self.lens_store),
                    history: history.clone(),
                };
                index_batch(
                    &collection,
                    &datastore,
                    &systemstore,
                    plan,
                    &mut source,
                    *max_docs,
                )
                .await
            };
            let batch = match outcome {
                Ok(batch) => batch,
                Err(error) => {
                    txn.discard()?;
                    return Err(error);
                }
            };
            match txn.commit().await {
                Ok(()) => {
                    *max_docs = (*max_docs * 2).min(BACKFILL_BATCH_DOCS);
                    return Ok(batch);
                }
                Err(error)
                    if error.is_txn_conflict()
                        && (*max_docs > 1 || retries_at_one < max_retries) =>
                {
                    if *max_docs > 1 {
                        *max_docs /= 2;
                    } else {
                        retries_at_one += 1;
                    }
                    tracing::debug!(
                        collection_id = %plan.collection_id,
                        index_id = plan.index_id,
                        last,
                        max_docs = *max_docs,
                        "index backfill batch conflicted with a concurrent write"
                    );
                }
                Err(error) => return Err(error),
            }
        }
    }
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
        let target = self.collection.version_id();
        let Some(version) = doc.schema_version_id().map(str::to_string) else {
            return Ok(Some((doc_short_id, doc)));
        };
        if !self.history.contains_key(&version)
            || !migration_path_has_transform(&self.history, &version, target)
        {
            return Ok(Some((doc_short_id, doc)));
        }
        let lens_doc = doc_to_lens_doc(&doc).ok_or_else(|| {
            IndexError::Other(format!("document {:?} cannot be migrated", doc.id()))
        })?;
        let mut lens = Lens::new(Arc::clone(&self.lens_store), target, self.history.clone());
        lens.put(&version, lens_doc)
            .await
            .map_err(|e| IndexError::Other(e.to_string()))?;
        let migrated = lens
            .next()
            .await
            .ok_or_else(|| {
                IndexError::Other(format!(
                    "lens produced no document while migrating {:?}",
                    doc.id()
                ))
            })?
            .map_err(|e| IndexError::Other(e.to_string()))?;
        let migrated = lens_doc_to_document(migrated, &doc, &self.collection);
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
