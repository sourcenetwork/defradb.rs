use async_trait::async_trait;
use bytes::Bytes;
use cid::Cid;
use document::{DocID, Document};
use query::mutator::{
    CreateResult, DeleteResult, DocMutator, MutationBatchController, UpdateResult,
};
use rapidhash::RapidHashSet;
use std::sync::Arc;
use storage::corekv::{OnCommit, Store};

use super::helpers::{
    ensure_collection_is_active, register_block_doc_id_mappings, register_created_doc,
    write_branchable_collection_block, write_local_create, write_local_update,
};
use crate::block::builder::DocRef;
use crate::block::builder::{write_delete_block_with_inputs, write_document_blocks_with_inputs};
use crate::collection::loader::{get_collection_with_index_manager, get_collection_with_lazy_load};
use crate::database::DB;
use crate::event::emission::register_update_event_callback;
use crate::txn::held::Held;
use crate::txn::{DbTxn, LentTxn};
use datastore::BasicTxn;
use defra_core::RequestContext;

pub struct BatchMutator<S: Store + 'static> {
    db: Arc<DB<S>>,
    prepared_inputs: Option<query::prepared::PreparedInputMemo>,
    txn: Held<S>,
    acp_registration_projection: Arc<query::txn::AcpRegistrationProjection>,
    /// The run of its request this mutator's attempt is.
    update_run: query::update_parts::Run,
}

impl<S: Store + 'static> BatchMutator<S> {
    pub async fn new(db: Arc<DB<S>>, txn: LentTxn<S>) -> Self {
        let projection = txn.acp_registration_projection();
        Self::new_with_prepared_inputs(db, txn, None, projection)
    }

    pub(crate) fn new_retry_controller(
        db: Arc<DB<S>>,
        prepared_inputs: Option<query::prepared::PreparedInputMemo>,
    ) -> Self {
        Self {
            db,
            prepared_inputs,
            txn: Held::none(),
            acp_registration_projection: query::txn::AcpRegistrationProjection::detached_shared(),
            update_run: query::update_parts::Run::First,
        }
    }

    pub(crate) fn new_with_prepared_inputs(
        db: Arc<DB<S>>,
        txn: LentTxn<S>,
        prepared_inputs: Option<query::prepared::PreparedInputMemo>,
        acp_registration_projection: Arc<query::txn::AcpRegistrationProjection>,
    ) -> Self {
        Self {
            db,
            txn: Held::new(txn),
            prepared_inputs,
            acp_registration_projection,
            update_run: query::update_parts::Run::First,
        }
    }

    async fn block_and_head_stores(
        &self,
    ) -> query::error::Result<(datastore::NamespaceView, datastore::NamespaceView)> {
        let txn = &*self.shared_txn()?;
        let blockstore = txn.blockstore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get blockstore: {}", e))
        })?;
        let headstore = txn.headstore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get headstore: {}", e))
        })?;
        Ok((blockstore, headstore))
    }

    async fn validate_collection_for_write(
        &self,
        collection: &crate::collection::Collection,
    ) -> query::error::Result<()> {
        let txn = &*self.shared_txn()?;
        txn.validate_collection_for_write(collection)
            .await
            .map_err(crate::error::commit_query_error)
    }

    pub async fn take_txn(&self) -> query::error::Result<DbTxn<S>> {
        self.txn.take().ok_or_else(|| {
            query::error::QueryError::execution("mutation batch transaction is no longer active")
        })
    }

    /// The transaction, for one operation.
    fn shared_txn(&self) -> query::error::Result<LentTxn<S>> {
        self.txn.get().ok_or_else(|| {
            query::error::QueryError::execution("mutation batch transaction is no longer active")
        })
    }
}

impl<S: Store + 'static> BatchMutator<S> {
    async fn check_attempt_node_access(
        &self,
        context: &RequestContext,
        permission: acp::nac::NodePermission,
    ) -> query::error::Result<()> {
        let Some(inputs) = &self.prepared_inputs else {
            return self
                .db
                .check_node_access_as(context, permission)
                .await
                .map_err(|error| query::error::QueryError::permission_denied(error.to_string()));
        };
        let identity = context
            .identity()
            .and_then(|value| identity::Did::new(value).ok())
            .unwrap_or_else(identity::Did::wildcard);
        let exact_request = serde_json::to_vec(&(identity.to_string(), permission.as_str()))
            .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
        let request = query::prepared::PreparedInputRequest::new(
            query::prepared::PreparedInputKind::NodePermission,
            exact_request,
        );
        let Some(bytes) = inputs.get(&request) else {
            return Err(query::error::QueryError::NeedsInput(request));
        };
        let allowed: bool = serde_json::from_slice(&bytes).map_err(|_| {
            query::error::QueryError::execution("invalid prepared node permission response")
        })?;
        if allowed {
            Ok(())
        } else {
            Err(query::error::QueryError::permission_denied(format!(
                "not authorized for node permission {}",
                permission.as_str()
            )))
        }
    }

    async fn register_created_doc_with_acp(
        &self,
        context: &RequestContext,
        collection: &crate::collection::Collection,
        doc_id: String,
    ) -> query::error::Result<()> {
        let txn = &*self.shared_txn()?;
        super::AutoCommitMutator::new(Arc::clone(&self.db))
            .register_created_doc_with_acp_as(context, txn, collection, doc_id)
    }

    #[allow(clippy::too_many_arguments)]
    async fn register_update_callback(
        &self,
        collection_name: String,
        collection_id: String,
        doc_id: String,
        doc_cid: Cid,
        doc_block: Bytes,
        collection_block: Option<(Cid, Bytes)>,
    ) -> query::error::Result<()> {
        let txn = &*self.shared_txn()?;
        register_update_event_callback(
            txn,
            self.db.event_bus(),
            // BatchMutator is the auto-commit-batch path; broadcast is handled
            // by the BroadcastMutator wrapper at the per-mutation layer.
            None,
            self.db.local_commit_release(),
            collection_name,
            collection_id,
            doc_id,
            doc_cid,
            doc_block,
            None,
            collection_block,
            None,
        )
        .map_err(|e| {
            query::error::QueryError::execution(format!(
                "failed to register tx update callback: {}",
                e
            ))
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static> MutationBatchController for BatchMutator<S> {
    async fn run(
        &self,
        operation: query::mutator::MutationBatchOperation,
        _mutator: Option<Arc<dyn DocMutator>>,
        _fetcher: Option<Arc<dyn query::fetcher::DocFetcher>>,
    ) -> query::error::Result<serde_json::Value> {
        let attempt = BatchRetryAttempt {
            db: self.db.clone(),
            prepared_inputs: self.prepared_inputs.clone(),
            operation,
        };
        match self
            .db
            .store()
            .transact_retry(self.db.options().retry_options(), Box::new(attempt))
            .await
        {
            Ok(value) => value
                .downcast::<serde_json::Value>()
                .map(|value| *value)
                .map_err(|_| {
                    query::error::QueryError::execution("invalid mutation batch retry result")
                }),
            Err(error) => match error.downcast_abort::<query::error::QueryError>() {
                Ok(error) => Err(error),
                Err(error) => Err(crate::error::commit_query_error(
                    crate::error::Error::Storage(error.into_storage()),
                )),
            },
        }
    }

    async fn commit(&self) -> query::error::Result<()> {
        let txn = self.take_txn().await?;
        let result = txn.commit().await.map_err(crate::error::commit_query_error);
        result
    }

    async fn discard(&self) -> query::error::Result<()> {
        let txn = self.take_txn().await?;
        let result = txn
            .discard()
            .map_err(|e| query::error::QueryError::execution(format!("discard error: {}", e)));
        result
    }
}

struct BatchRetryAttempt<S: Store> {
    db: Arc<DB<S>>,
    prepared_inputs: Option<query::prepared::PreparedInputMemo>,
    operation: query::mutator::MutationBatchOperation,
}

impl<S: Store + 'static> storage::corekv::RetryAttemptFn for BatchRetryAttempt<S> {
    fn run<'a>(
        &'a mut self,
        retry: &'a mut dyn storage::corekv::RetryTxn,
    ) -> storage::corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            let io = retry
                .begin_attempt_io()
                .map_err(storage::corekv::RetryAttemptError::Storage)?;
            let basic_txn = BasicTxn::from_retry_attempt_io(io, 0, self.db.crdts().clone());
            let txn: LentTxn<S> = Arc::new(self.db.new_db_txn(basic_txn));
            let acp_registration_projection = txn.acp_registration_projection();
            let fetcher = Arc::new(
                crate::LensedDocFetcher::lent(
                    self.db.clone(),
                    &txn,
                    self.db.lens_store.clone(),
                    Default::default(),
                )
                .with_prepared_inputs(self.prepared_inputs.clone()),
            );
            let batch = Arc::new(BatchMutator {
                db: self.db.clone(),
                txn: Held::new(txn),
                prepared_inputs: self.prepared_inputs.clone(),
                acp_registration_projection,
                update_run: query::update_parts::Run::after(retry.previous_conflict().as_ref()),
            });
            let mutator: Arc<dyn DocMutator> = batch.clone();
            let fetcher_dyn: Arc<dyn query::fetcher::DocFetcher> = fetcher.clone();
            let outcome = (self.operation)(mutator, fetcher_dyn).await;
            drop(fetcher);

            let value = match outcome {
                Ok(value) => value,
                Err(error) => {
                    if let Ok(txn) = batch.take_txn().await {
                        let _ = txn.discard();
                    }
                    return Err(storage::corekv::RetryAttemptError::Abort(Box::new(error)));
                }
            };

            let txn = batch
                .take_txn()
                .await
                .map_err(|error| storage::corekv::RetryAttemptError::Abort(Box::new(error)))?;
            let (io, callbacks) = txn
                .into_retry_attempt()
                .map_err(storage::corekv::RetryAttemptError::Storage)?;
            drop(batch);
            retry
                .finish_attempt_io(io, callbacks)
                .map_err(storage::corekv::RetryAttemptError::Storage)?;
            Ok(Box::new(value) as storage::corekv::RetryAttemptValue)
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: Store + 'static> DocMutator for BatchMutator<S> {
    fn crdts(&self) -> crdt::Crdts {
        self.db.crdts().clone()
    }

    fn supports_projected_updates(&self) -> bool {
        true
    }

    fn update_run(&self) -> query::update_parts::Run {
        self.update_run
    }

    fn set_document_acp(&self, provider: Arc<dyn acp::DocumentACP>) {
        self.db.set_document_acp(provider);
    }

    fn manages_external_acp_hooks(&self) -> bool {
        true
    }

    fn acp_registration_projection(&self) -> Option<Arc<query::txn::AcpRegistrationProjection>> {
        Some(self.acp_registration_projection.clone())
    }

    async fn register_ordered_async_hook(
        &self,
        callback: storage::corekv::CommitHook,
    ) -> query::error::Result<()> {
        let txn = &*self.shared_txn()?;
        txn.on_commit(OnCommit::OrderedAsync(callback))
            .map_err(|error| query::error::QueryError::execution(error.to_string()))
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
        mut doc: Document,
    ) -> query::error::Result<CreateResult> {
        self.check_attempt_node_access(context, acp::nac::NodePermission::DocumentUpdate)
            .await?;

        let (collection, datastore, systemstore, index_manager) =
            get_collection_with_index_manager(&*self.shared_txn()?, collection_name).await?;
        self.db
            .check_node_access_in_txn(
                context,
                systemstore.sibling(storage::namespace::Namespace::Acpstore),
                acp::nac::NodePermission::DocumentUpdate,
            )
            .await
            .map_err(crate::error::commit_query_error)?;
        self.validate_collection_for_write(&collection).await?;
        ensure_collection_is_active(&self.db, collection_name, &collection)?;
        let embedding_config = self.db.options().embedding_config();

        let prepared_inputs = self.prepared_inputs.clone().unwrap_or_default();
        crate::search::embedding::set_embedding_from_inputs(
            &collection.schema().vector_embeddings,
            &mut doc,
            true,
            &RapidHashSet::default(),
            &embedding_config,
            &prepared_inputs,
        )?;

        // No per-doc write guard for creates: the DocID is derived from the
        // genesis block inside the txn; the mapping duplicate check is the gate.

        self.db
            .validate_downsample_write(&datastore, &systemstore, &collection, &doc, None)
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;

        let short_id = collection.resolved_root_id();
        let enc_config = context.encryption();
        let sign_config = context.signing();

        let identity = match crate::write::prepare::prepared_identity(&doc) {
            Some(identity) => identity,
            None => DocRef::new(
                short_id,
                self.db
                    .next_doc_short_id()
                    .await
                    .map_err(|e| query::error::QueryError::execution(e.to_string()))?,
            ),
        };
        let doc_short_id = identity.doc_short_id;
        {
            let (blockstore, headstore) = self.block_and_head_stores().await?;
            crate::write::prepare::prepare_keys_with_stores(
                &self.db,
                &mut doc,
                identity,
                None,
                enc_config,
                Some((&blockstore, &headstore)),
                None,
            )
            .await?;
        }

        let (doc_id, doc_cid, doc_block, col_block_data) = {
            let (blockstore, headstore) = self.block_and_head_stores().await?;

            let block_result = write_document_blocks_with_inputs(
                &blockstore,
                &headstore,
                &doc,
                collection.schema(),
                identity,
                None,
                enc_config,
                sign_config,
                context.batch_session(),
                self.db.kms().as_ref(),
                self.prepared_inputs.as_ref(),
                None,
            )
            .await
            .map_err(|error| {
                error.into_query_error_in(&format!(
                    "failed to write document blocks for create on collection {collection_name}"
                ))
            })?;

            let doc_id = register_created_doc(
                self.db.document_acp(),
                context.creator(),
                &systemstore,
                &datastore,
                &collection,
                doc_short_id,
                &block_result,
            )
            .await?;
            self.register_created_doc_with_acp(context, &collection, doc_id.to_string())
                .await?;
            doc.set_id(doc_id.clone());

            write_local_create(&datastore, &collection, &doc, doc_short_id, &index_manager).await?;

            let col_block_data = write_branchable_collection_block(
                &self.db,
                collection_name,
                &collection,
                &blockstore,
                &headstore,
                block_result.cid,
                doc_short_id,
                context,
                self.prepared_inputs.as_ref(),
            )
            .await?;

            (doc_id, block_result.cid, block_result.block, col_block_data)
        };

        self.register_update_callback(
            collection_name.to_string(),
            collection.collection_id().to_string(),
            doc_id.to_string(),
            doc_cid,
            doc_block.clone(),
            col_block_data.clone(),
        )
        .await?;

        doc.clear_write_preparation();
        let mut result = CreateResult::with_commit(doc_id, doc, doc_cid, doc_block);
        if let Some((col_cid, col_bytes)) = col_block_data {
            result.broadcast_cid = Some(col_cid);
            result.broadcast_block = Some(col_bytes);
        }
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
        mut doc: Document,
        mut modified_fields: RapidHashSet<String>,
    ) -> query::error::Result<UpdateResult> {
        self.check_attempt_node_access(context, acp::nac::NodePermission::DocumentUpdate)
            .await?;

        let (collection, datastore, systemstore, index_manager) =
            get_collection_with_index_manager(&*self.shared_txn()?, collection_name).await?;
        self.db
            .check_node_access_in_txn(
                context,
                systemstore.sibling(storage::namespace::Namespace::Acpstore),
                acp::nac::NodePermission::DocumentUpdate,
            )
            .await
            .map_err(crate::error::commit_query_error)?;
        self.validate_collection_for_write(&collection).await?;
        ensure_collection_is_active(&self.db, collection_name, &collection)?;
        let embedding_config = self.db.options().embedding_config();
        let prepared_inputs = self.prepared_inputs.clone().unwrap_or_default();

        let doc_id = doc
            .id()
            .cloned()
            .ok_or_else(|| query::error::QueryError::execution("update requires a document ID"))?;
        let (doc_short_id, canonical_doc_id) = collection
            .require_doc_identity(&systemstore, &doc_id)
            .await
            .map_err(|e| match e {
                crate::error::Error::DocumentNotFound(id) => {
                    query::error::QueryError::document_not_found(id)
                }
                other => query::error::QueryError::execution(other.to_string()),
            })?;
        doc.set_id(canonical_doc_id.clone());

        if context.encryption().is_none()
            && !self
                .db
                .is_downsample_source(collection_name)
                .map_err(|error| query::error::QueryError::execution(error.to_string()))?
            && collection
                .retain_changed_fields(
                    &datastore,
                    doc_short_id,
                    &doc,
                    &mut modified_fields,
                    self.update_run,
                )
                .await
                .map_err(crate::error::commit_query_error)?
        {
            doc.clear_write_preparation();
            return Ok(UpdateResult::new(doc, 0));
        }

        self.db
            .validate_downsample_write(
                &datastore,
                &systemstore,
                &collection,
                &doc,
                Some(&modified_fields),
            )
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;

        let heads = super::helpers::load_update_heads(&datastore, doc_short_id).await?;
        write_local_update(
            &datastore,
            &collection,
            &mut doc,
            doc_short_id,
            &index_manager,
            &mut modified_fields,
            &heads,
            self.update_run,
            &embedding_config,
            &prepared_inputs,
        )
        .await?;

        let short_id = collection.resolved_root_id();
        let enc_config = context.encryption();
        let sign_config = context.signing();

        {
            let (blockstore, headstore) = self.block_and_head_stores().await?;
            crate::write::prepare::prepare_keys_with_stores(
                &self.db,
                &mut doc,
                DocRef::new(short_id, doc_short_id),
                Some(&modified_fields),
                enc_config,
                Some((&blockstore, &headstore)),
                None,
            )
            .await?;
        }

        let (doc_cid, doc_block, col_block_data) = {
            let (blockstore, headstore) = self.block_and_head_stores().await?;

            let block_result = write_document_blocks_with_inputs(
                &blockstore,
                &headstore,
                &doc,
                collection.schema(),
                DocRef::new(short_id, doc_short_id),
                Some(&modified_fields),
                enc_config,
                sign_config,
                context.batch_session(),
                self.db.kms().as_ref(),
                self.prepared_inputs.as_ref(),
                Some(heads),
            )
            .await
            .map_err(|error| {
                error.into_query_error_in(&format!(
                    "failed to write document blocks for update on collection {collection_name}"
                ))
            })?;

            register_block_doc_id_mappings(
                &systemstore,
                &block_result,
                &canonical_doc_id.to_string(),
            )
            .await?;

            let col_block_data = write_branchable_collection_block(
                &self.db,
                collection_name,
                &collection,
                &blockstore,
                &headstore,
                block_result.cid,
                doc_short_id,
                context,
                self.prepared_inputs.as_ref(),
            )
            .await?;

            (block_result.cid, block_result.block, col_block_data)
        };

        if let Some(doc_id) = doc.id() {
            self.register_update_callback(
                collection_name.to_string(),
                collection.collection_id().to_string(),
                doc_id.to_string(),
                doc_cid,
                doc_block.clone(),
                col_block_data.clone(),
            )
            .await?;
        }

        let fields_modified = modified_fields.len();
        doc.clear_write_preparation();
        let mut result = UpdateResult::with_commit(doc, fields_modified, doc_cid, doc_block);
        if let Some((col_cid, col_bytes)) = col_block_data {
            result.broadcast_cid = Some(col_cid);
            result.broadcast_block = Some(col_bytes);
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
        self.check_attempt_node_access(context, acp::nac::NodePermission::DocumentDelete)
            .await?;

        let (collection, datastore, systemstore, index_manager) =
            get_collection_with_index_manager(&*self.shared_txn()?, collection_name).await?;
        self.db
            .check_node_access_in_txn(
                context,
                systemstore.sibling(storage::namespace::Namespace::Acpstore),
                acp::nac::NodePermission::DocumentDelete,
            )
            .await
            .map_err(crate::error::commit_query_error)?;
        self.validate_collection_for_write(&collection).await?;
        ensure_collection_is_active(&self.db, collection_name, &collection)?;

        let Some((doc_short_id, canonical_doc_id)) = collection
            .resolve_doc_identity(&systemstore, doc_id)
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?
        else {
            return Ok(DeleteResult::new(doc_id.clone(), false));
        };

        let existed = collection
            .delete_with_indexes(&datastore, &canonical_doc_id, doc_short_id, &index_manager)
            .await
            .map_err(|e| query::error::QueryError::execution(format!("delete error: {}", e)))?;

        if !existed {
            return Ok(DeleteResult::new(canonical_doc_id, existed));
        }
        crate::write::autocommit::helpers::unregister_deleted_doc(
            self.db.document_acp(),
            &systemstore,
            &collection,
            &canonical_doc_id,
        )
        .await?;

        let collection_version_id = collection.version_id();
        let sign_config = context.signing();

        let (doc_cid, doc_block, col_block_data) = {
            let (blockstore, headstore) = self.block_and_head_stores().await?;

            let block_result = write_delete_block_with_inputs(
                &blockstore,
                &headstore,
                &canonical_doc_id.to_string(),
                doc_short_id,
                collection_version_id,
                sign_config,
                context.batch_session(),
                self.prepared_inputs.as_ref(),
                None,
            )
            .await
            .map_err(|error| {
                error.into_query_error_in(&format!(
                    "failed to write delete block for collection {collection_name}"
                ))
            })?;

            let composite_cid = block_result.cid;

            crate::docid::map::set_block_doc_id_mapping(
                &systemstore,
                &composite_cid.to_string(),
                &canonical_doc_id.to_string(),
            )
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;

            let col_block_data = write_branchable_collection_block(
                &self.db,
                collection_name,
                &collection,
                &blockstore,
                &headstore,
                composite_cid,
                doc_short_id,
                context,
                self.prepared_inputs.as_ref(),
            )
            .await?;

            (composite_cid, block_result.block, col_block_data)
        };

        {
            let txn = &*self.shared_txn()?;
            super::AutoCommitMutator::new(Arc::clone(&self.db)).register_deleted_doc_with_acp_as(
                context,
                txn,
                &collection,
                canonical_doc_id.to_string(),
            )?;
        }

        self.register_update_callback(
            collection_name.to_string(),
            collection.collection_id().to_string(),
            canonical_doc_id.to_string(),
            doc_cid,
            doc_block.clone(),
            col_block_data.clone(),
        )
        .await?;

        let mut result = DeleteResult::with_commit(canonical_doc_id, existed, doc_cid, doc_block);
        if let Some((col_cid, col_bytes)) = col_block_data {
            result.broadcast_cid = Some(col_cid);
            result.broadcast_block = Some(col_bytes);
        }
        Ok(result)
    }

    async fn exists(&self, collection_name: &str, doc_id: &DocID) -> query::error::Result<bool> {
        let (collection, datastore, systemstore) =
            get_collection_with_lazy_load(&*self.shared_txn()?, collection_name).await?;

        collection
            .exists_by_doc_id(&datastore, &systemstore, doc_id)
            .await
            .map_err(|e| query::error::QueryError::execution(format!("exists error: {}", e)))
    }

    async fn get_for_update(
        &self,
        collection_name: &str,
        doc_id: &DocID,
    ) -> query::error::Result<Option<Document>> {
        let (collection, datastore, systemstore) =
            get_collection_with_lazy_load(&*self.shared_txn()?, collection_name).await?;

        collection
            .get_by_doc_id(&datastore, &systemstore, doc_id)
            .await
            .map_err(|e| {
                query::error::QueryError::execution(format!("get_for_update error: {}", e))
            })
    }
}
