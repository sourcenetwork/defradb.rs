use async_trait::async_trait;
use blockstore::Blockstore;
use bytes::Bytes;
use cid::Cid;
use defra_core::RequestContext;
use document::{DocID, Document};
use kovan_queue::seg_queue::SegQueue;
use p2p::sync::SyncCoordinator;
use p2p::transport::P2PTransport;
use query::mutator::{
    BroadcastStatus, CreateResult, DeleteResult, DocMutator, MutationBatchController, UpdateResult,
};
use rapidhash::RapidHashSet;
use std::sync::Arc;
use storage::corekv::Store;
use tracing::{error, warn};

use crate::block::builder::BlockResult;
use crate::database::DB;

fn document_json_value(doc: &Document) -> Option<serde_json::Value> {
    Some(serde_json::Value::Object(
        doc.to_map().ok()?.into_iter().collect(),
    ))
}

struct PendingBroadcast {
    cid: Cid,
    block: Bytes,
    doc_id: String,
    collection_id: String,
    collection_name: String,
    document_json: Option<serde_json::Value>,
    creator_did: Option<String>,
    se_artifacts: Vec<p2p::message::SEArtifact>,
    broadcast_cid: Option<Cid>,
    broadcast_block: Option<Bytes>,
}

struct BroadcastCapture<'a> {
    collection_name: &'a str,
    creator_did: Option<String>,
    doc_id: &'a str,
    commit_cid: Option<Cid>,
    commit_block: Option<&'a Bytes>,
    document_json: Option<serde_json::Value>,
    se_artifacts: Vec<p2p::message::SEArtifact>,
    broadcast_cid: Option<Cid>,
    broadcast_block: Option<&'a Bytes>,
}

pub(crate) struct BroadcastBatchMutator<S: Store, B: Blockstore, T: P2PTransport> {
    inner: Arc<dyn DocMutator>,
    sync: Arc<SyncCoordinator<B, T>>,
    db: Arc<DB<S>>,
    pending_broadcasts: SegQueue<PendingBroadcast>,
    inner_controller: Arc<dyn MutationBatchController>,
    se_options: super::BroadcastSeOptions,
}

impl<S: Store + 'static, B: Blockstore + 'static, T: P2PTransport + 'static>
    BroadcastBatchMutator<S, B, T>
{
    pub(crate) fn new(
        inner: Arc<dyn DocMutator>,
        inner_controller: Arc<dyn MutationBatchController>,
        sync: Arc<SyncCoordinator<B, T>>,
        db: Arc<DB<S>>,
        se_options: super::BroadcastSeOptions,
    ) -> Self {
        Self {
            inner,
            sync,
            db,
            pending_broadcasts: SegQueue::new(),
            inner_controller,
            se_options,
        }
    }

    fn drain_pending_broadcasts(&self) -> Vec<PendingBroadcast> {
        let mut drained = Vec::with_capacity(self.pending_broadcasts.len());
        while let Some(pending) = self.pending_broadcasts.pop() {
            drained.push(pending);
        }
        drained
    }

    fn get_collection_id(&self, collection_name: &str) -> query::error::Result<String> {
        let collection = self
            .db
            .get_collection(collection_name)
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?
            .ok_or_else(|| query::error::QueryError::collection_not_found(collection_name))?;
        Ok(collection.collection_id().to_string())
    }

    fn generate_se_artifacts(
        &self,
        collection_name: &str,
        doc: &Document,
        fields: &[String],
    ) -> query::error::Result<Vec<p2p::message::SEArtifact>> {
        let collection = self
            .db
            .get_collection(collection_name)
            .map_err(|error| query::error::QueryError::execution(error.to_string()))?
            .ok_or_else(|| query::error::QueryError::collection_not_found(collection_name))?;
        let Some(doc_id) = doc.id() else {
            return Ok(Vec::new());
        };
        Ok(
            super::BroadcastMutator::<S, B, T>::generate_se_artifacts_with_options(
                self.se_options.clone(),
                collection.schema(),
                &doc_id.to_string(),
                doc,
                fields,
            ),
        )
    }

    async fn capture_broadcast(&self, capture: BroadcastCapture<'_>) -> query::error::Result<()> {
        let BroadcastCapture {
            collection_name,
            creator_did,
            doc_id,
            commit_cid,
            commit_block,
            document_json,
            se_artifacts,
            broadcast_cid,
            broadcast_block,
        } = capture;

        let (cid, block) = match (commit_cid, commit_block) {
            (Some(cid), Some(block)) => (cid, block.clone()),
            _ => {
                warn!(
                    collection = %collection_name,
                    doc_id = %doc_id,
                    "Missing commit block data for batched broadcast; skipping deferred broadcast"
                );
                return Ok(());
            }
        };

        let collection_id = self.get_collection_id(collection_name)?;
        self.pending_broadcasts.push(PendingBroadcast {
            cid,
            block,
            doc_id: doc_id.to_string(),
            collection_id,
            collection_name: collection_name.to_string(),
            document_json,
            se_artifacts,
            creator_did,
            broadcast_cid,
            broadcast_block: broadcast_block.cloned(),
        });

        Ok(())
    }

    async fn register_pending_static(
        sync: &SyncCoordinator<B, T>,
        pending: &PendingBroadcast,
    ) -> p2p::error::Result<()> {
        let creator_ref = pending.creator_did.as_deref();
        if let Some(document_json) = pending.document_json.as_ref() {
            sync.push_document_to_replicators_with_creator(
                &pending.cid,
                &pending.block,
                &pending.doc_id,
                &pending.collection_id,
                document_json,
                creator_ref,
            )
            .await?;
        } else {
            sync.push_to_replicators_with_creator(
                &pending.cid,
                &pending.block,
                &pending.doc_id,
                &pending.collection_id,
                creator_ref,
            )
            .await?;
        }

        if let (Some(col_cid), Some(col_block)) = (
            pending.broadcast_cid.as_ref(),
            pending.broadcast_block.as_ref(),
        ) {
            sync.push_to_replicators_with_creator(
                col_cid,
                col_block,
                "",
                &pending.collection_id,
                creator_ref,
            )
            .await?;
        }
        Ok(())
    }

    async fn broadcast_pending_static(
        sync: &SyncCoordinator<B, T>,
        bus: Option<&Arc<dyn events::Bus>>,
        pending: PendingBroadcast,
    ) {
        let PendingBroadcast {
            cid,
            block,
            doc_id,
            collection_id,
            collection_name,
            document_json,
            se_artifacts,
            creator_did,
            broadcast_cid,
            broadcast_block,
        } = pending;

        if let Some(document_json) = &document_json {
            sync.push_se_artifacts_to_replicators_for_document(
                &collection_id,
                se_artifacts,
                document_json,
            )
            .await;
        }

        let creator_ref = creator_did.as_deref();

        let block_result = BlockResult {
            cid,
            block,
            doc_id: doc_id.clone(),
            field_cids: vec![],
            encryption_cids: vec![],
        };
        let broadcast_status = super::broadcast::broadcast_with_retry_with_creator(
            sync,
            bus,
            &doc_id,
            &block_result,
            &collection_id,
            &collection_name,
            creator_ref,
        )
        .await;

        if let BroadcastStatus::Failed(error) = &broadcast_status {
            error!(
                doc_id = %doc_id,
                collection = %collection_name,
                error = %error,
                "Deferred batch broadcast failed — document committed locally but NOT replicated"
            );
        }

        if let (Some(col_cid), Some(col_block)) = (broadcast_cid, broadcast_block) {
            let col_block_result = BlockResult {
                cid: col_cid,
                block: col_block,
                doc_id: String::new(),
                field_cids: vec![],
                encryption_cids: vec![],
            };
            let collection_broadcast_status = super::broadcast::broadcast_with_retry_with_creator(
                sync,
                bus,
                &doc_id,
                &col_block_result,
                &collection_id,
                &collection_name,
                creator_ref,
            )
            .await;

            if let BroadcastStatus::Failed(error) = &collection_broadcast_status {
                error!(
                    doc_id = %col_block_result.doc_id,
                    collection = %collection_name,
                    error = %error,
                    "Deferred branchable collection broadcast failed — NOT replicated"
                );
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static, B: Blockstore + 'static, T: P2PTransport> DocMutator
    for BroadcastBatchMutator<S, B, T>
{
    fn crdts(&self) -> crdt::Crdts {
        self.db.crdts().clone()
    }

    fn engine_retries_batches(&self) -> bool {
        self.inner.engine_retries_batches()
    }

    fn acp_registration_projection(&self) -> Option<Arc<query::txn::AcpRegistrationProjection>> {
        self.inner.acp_registration_projection()
    }

    fn requires_write_preparation(&self) -> bool {
        self.requires_write_preparation_as(&RequestContext::ambient())
    }

    fn requires_write_preparation_as(&self, context: &RequestContext) -> bool {
        self.inner.requires_write_preparation_as(context)
    }

    fn supports_projected_updates(&self) -> bool {
        self.inner.supports_projected_updates()
    }

    fn update_run(&self) -> query::update_parts::Run {
        self.inner.update_run()
    }

    async fn prepare_write(
        &self,
        collection_name: &str,
        doc: Document,
        modified_fields: Option<RapidHashSet<String>>,
        encryption: Option<defra_core::encryption::EncryptionConfig>,
    ) -> query::error::Result<Document> {
        self.prepare_write_as(
            &RequestContext::ambient().or_encryption(encryption),
            collection_name,
            doc,
            modified_fields,
        )
        .await
    }

    async fn prepare_write_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        doc: Document,
        modified_fields: Option<RapidHashSet<String>>,
    ) -> query::error::Result<Document> {
        self.inner
            .prepare_write_as(context, collection_name, doc, modified_fields)
            .await
    }

    async fn prepare_creates_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        docs: Vec<Document>,
    ) -> query::error::Result<Vec<Document>> {
        self.inner
            .prepare_creates_as(context, collection_name, docs)
            .await
    }

    async fn resolve_prepared_input(
        &self,
        request: &query::prepared::PreparedInputRequest,
    ) -> query::error::Result<Arc<[u8]>> {
        self.resolve_prepared_input_as(&RequestContext::ambient(), request)
            .await
    }

    async fn resolve_prepared_input_as(
        &self,
        context: &RequestContext,
        request: &query::prepared::PreparedInputRequest,
    ) -> query::error::Result<Arc<[u8]>> {
        self.inner.resolve_prepared_input_as(context, request).await
    }

    async fn resolve_prepared_inputs(
        &self,
        requests: Vec<query::prepared::PreparedInputRequest>,
    ) -> query::error::Result<Vec<Arc<[u8]>>> {
        self.resolve_prepared_inputs_as(&RequestContext::ambient(), requests)
            .await
    }

    async fn resolve_prepared_inputs_as(
        &self,
        context: &RequestContext,
        requests: Vec<query::prepared::PreparedInputRequest>,
    ) -> query::error::Result<Vec<Arc<[u8]>>> {
        self.inner
            .resolve_prepared_inputs_as(context, requests)
            .await
    }

    async fn prepared_embedding_request(
        &self,
        collection_name: &str,
        field_name: &str,
        doc: &Document,
    ) -> query::error::Result<Option<query::prepared::PreparedInputRequest>> {
        self.inner
            .prepared_embedding_request(collection_name, field_name, doc)
            .await
    }

    fn set_document_acp(&self, acp: Arc<dyn acp::DocumentACP>) {
        self.inner.set_document_acp(acp);
    }

    fn manages_external_acp_hooks(&self) -> bool {
        self.inner.manages_external_acp_hooks()
    }

    async fn register_ordered_async_hook(
        &self,
        callback: storage::corekv::CommitHook,
    ) -> query::error::Result<()> {
        self.inner.register_ordered_async_hook(callback).await
    }

    fn with_prepared_inputs(
        &self,
        inputs: query::prepared::PreparedInputMemo,
    ) -> Option<Arc<dyn DocMutator>> {
        let inner = self.inner.with_prepared_inputs(inputs)?;
        Some(Arc::new(Self::new(
            inner,
            self.inner_controller.clone(),
            self.sync.clone(),
            self.db.clone(),
            self.se_options.clone(),
        )))
    }

    async fn create(
        &self,
        collection_name: &str,
        doc: Document,
    ) -> query::error::Result<CreateResult> {
        self.create_as(&RequestContext::ambient(), collection_name, doc)
            .await
    }

    async fn create_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        doc: Document,
    ) -> query::error::Result<CreateResult> {
        let result = self.inner.create_as(context, collection_name, doc).await?;
        let doc_id = result.doc_id.to_string();
        let document_json = document_json_value(&result.document);
        self.capture_broadcast(BroadcastCapture {
            collection_name,
            creator_did: context.creator().map(str::to_owned),
            doc_id: &doc_id,
            commit_cid: result.commit_cid,
            commit_block: result.commit_block.as_ref(),
            document_json,
            se_artifacts: self.generate_se_artifacts(collection_name, &result.document, &[])?,
            broadcast_cid: result.broadcast_cid,
            broadcast_block: result.broadcast_block.as_ref(),
        })
        .await?;
        Ok(result)
    }

    async fn update(
        &self,
        collection_name: &str,
        doc: Document,
        modified_fields: RapidHashSet<String>,
    ) -> query::error::Result<UpdateResult> {
        self.update_as(
            &RequestContext::ambient(),
            collection_name,
            doc,
            modified_fields,
        )
        .await
    }

    async fn update_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        doc: Document,
        modified_fields: RapidHashSet<String>,
    ) -> query::error::Result<UpdateResult> {
        let se_fields: Vec<String> = modified_fields.iter().cloned().collect();
        let result = self
            .inner
            .update_as(context, collection_name, doc, modified_fields)
            .await?;

        if let Some(doc_id) = result.document.id() {
            let doc_id = doc_id.to_string();
            let document_json = document_json_value(&result.document);
            self.capture_broadcast(BroadcastCapture {
                collection_name,
                creator_did: context.creator().map(str::to_owned),
                doc_id: &doc_id,
                commit_cid: result.commit_cid,
                commit_block: result.commit_block.as_ref(),
                document_json,
                se_artifacts: self.generate_se_artifacts(
                    collection_name,
                    &result.document,
                    &se_fields,
                )?,
                broadcast_cid: result.broadcast_cid,
                broadcast_block: result.broadcast_block.as_ref(),
            })
            .await?;
        } else {
            warn!(
                collection = %collection_name,
                "Updated document is missing _docID; skipping deferred broadcast"
            );
        }

        Ok(result)
    }

    async fn delete(
        &self,
        collection_name: &str,
        doc_id: &DocID,
    ) -> query::error::Result<DeleteResult> {
        self.delete_as(&RequestContext::ambient(), collection_name, doc_id)
            .await
    }

    async fn delete_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        doc_id: &DocID,
    ) -> query::error::Result<DeleteResult> {
        let document_json = self
            .inner
            .get_for_update(collection_name, doc_id)
            .await?
            .and_then(|doc| document_json_value(&doc));
        let result = self
            .inner
            .delete_as(context, collection_name, doc_id)
            .await?;
        let doc_id = result.doc_id.to_string();
        // Pass through the branchable collection block so it gets broadcast
        // alongside the composite delete (matches the non-batch BroadcastMutator
        // path and Go's two-update emit for branchable mutations).
        self.capture_broadcast(BroadcastCapture {
            collection_name,
            creator_did: context.creator().map(str::to_owned),
            doc_id: &doc_id,
            commit_cid: result.commit_cid,
            commit_block: result.commit_block.as_ref(),
            document_json,
            se_artifacts: Vec::new(),
            broadcast_cid: result.broadcast_cid,
            broadcast_block: result.broadcast_block.as_ref(),
        })
        .await?;
        Ok(result)
    }

    async fn exists(&self, collection_name: &str, doc_id: &DocID) -> query::error::Result<bool> {
        self.inner.exists(collection_name, doc_id).await
    }

    async fn get_for_update(
        &self,
        collection_name: &str,
        doc_id: &DocID,
    ) -> query::error::Result<Option<Document>> {
        self.inner.get_for_update(collection_name, doc_id).await
    }

    async fn is_deleted(
        &self,
        collection_name: &str,
        doc_id: &DocID,
    ) -> query::error::Result<bool> {
        self.inner.is_deleted(collection_name, doc_id).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static, B: Blockstore + 'static, T: P2PTransport> MutationBatchController
    for BroadcastBatchMutator<S, B, T>
{
    async fn run(
        &self,
        operation: query::mutator::MutationBatchOperation,
        mutator: Option<Arc<dyn DocMutator>>,
        fetcher: Option<Arc<dyn query::fetcher::DocFetcher>>,
    ) -> query::error::Result<serde_json::Value> {
        // The broadcasts of the attempt that succeeded last. Attempts run one
        // after another, so each one empties the queue before it offers its
        // own: the queue never holds more than the latest attempt's.
        let committed: Arc<SegQueue<Vec<PendingBroadcast>>> = Arc::new(SegQueue::new());
        let committed_for_attempt = committed.clone();
        let inner_controller = self.inner_controller.clone();
        let sync = self.sync.clone();
        let db = self.db.clone();
        let se_options = self.se_options.clone();
        let wrapped_operation: query::mutator::MutationBatchOperation =
            Arc::new(move |attempt_mutator, attempt_fetcher| {
                let wrapper = Arc::new(BroadcastBatchMutator::new(
                    attempt_mutator,
                    inner_controller.clone(),
                    sync.clone(),
                    db.clone(),
                    se_options.clone(),
                ));
                let wrapper_mutator: Arc<dyn DocMutator> = wrapper.clone();
                let operation = operation.clone();
                let committed = committed_for_attempt.clone();
                Box::pin(async move {
                    let result = operation(wrapper_mutator, attempt_fetcher).await;
                    if result.is_ok() {
                        while committed.pop().is_some() {}
                        committed.push(wrapper.drain_pending_broadcasts());
                    }
                    result
                })
            });

        let result = self
            .inner_controller
            .run(wrapped_operation, mutator, fetcher)
            .await?;
        let pending_broadcasts = committed.pop().unwrap_or_default();
        if !pending_broadcasts.is_empty() {
            let mut marker_errors = Vec::new();
            for pending in &pending_broadcasts {
                if let Err(error) = Self::register_pending_static(&self.sync, pending).await {
                    marker_errors.push(error.to_string());
                }
            }
            let bus = self.db.event_bus().cloned();
            let sync = self.sync.clone();
            self.sync.spawn_non_authoritative_broadcast_task(
                "broadcast_mutation_batch",
                async move {
                    for pending in pending_broadcasts {
                        Self::broadcast_pending_static(&sync, bus.as_ref(), pending).await;
                    }
                },
            );
            if !marker_errors.is_empty() {
                return Err(query::error::QueryError::execution(format!(
                    "committed batch has undurable P2P head markers: {}",
                    marker_errors.join("; ")
                )));
            }
        }
        Ok(result)
    }

    /// Commit the inner mutation batch, then durably register its outbound P2P
    /// markers. An error reporting `undurable P2P head markers` is post-commit:
    /// callers must not interpret it as a rolled-back mutation and retry the
    /// logical write blindly.
    async fn commit(&self) -> query::error::Result<()> {
        if let Err(err) = self.inner_controller.commit().await {
            self.drain_pending_broadcasts();
            return Err(err);
        }

        let pending_broadcasts = self.drain_pending_broadcasts();
        if !pending_broadcasts.is_empty() {
            let mut marker_errors = Vec::new();
            for pending in &pending_broadcasts {
                if let Err(error) = Self::register_pending_static(&self.sync, pending).await {
                    marker_errors.push(error.to_string());
                }
            }
            let bus = self.db.event_bus().cloned();
            let sync = self.sync.clone();
            self.sync.spawn_non_authoritative_broadcast_task(
                "broadcast_mutation_batch",
                async move {
                    for pending in pending_broadcasts {
                        Self::broadcast_pending_static(&sync, bus.as_ref(), pending).await;
                    }
                },
            );
            if !marker_errors.is_empty() {
                return Err(query::error::QueryError::execution(format!(
                    "committed batch has undurable P2P head markers: {}",
                    marker_errors.join("; ")
                )));
            }
        }

        Ok(())
    }

    async fn discard(&self) -> query::error::Result<()> {
        let discard_result = self.inner_controller.discard().await;
        self.drain_pending_broadcasts();
        discard_result
    }
}
