use super::helpers::{register_created_doc, write_branchable_collection_block, write_local_create};
use super::*;
use bytes::Bytes;
use events::{Message, Update};
use storage::corekv::OnCommit;

use crate::block::builder::DocRef;

use crate::block::builder::{
    compute_document_blocks_with_inputs, insert_computed_blocks, ComputedBlocks,
};
use crate::write::prepare::PreparedCreateBlocks;

#[cfg(feature = "native")]
mod runs;

struct AcpRegistration {
    acp: Arc<dyn acp::DocumentACP>,
    creator: identity::Did,
    policy_id: String,
    resource_name: String,
    bearer_token: Option<String>,
}

struct CreateRetryAttempt<S: Store> {
    db: Arc<DB<S>>,
    collection_name: String,
    collection: Collection,
    doc: Arc<Document>,
    computed_blocks: ComputedBlocks,
    doc_short_id: u64,
    context: RequestContext,
    acp_registration: Option<AcpRegistration>,
    prepared_inputs: Option<query::prepared::PreparedInputMemo>,
}

impl<S: Store + 'static> storage::corekv::RetryAttemptFn for CreateRetryAttempt<S> {
    fn run<'a>(
        &'a mut self,
        retry: &'a mut dyn storage::corekv::RetryTxn,
    ) -> storage::corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            let mutator = AutoCommitMutator {
                db: Arc::clone(&self.db),
                prepared_inputs: self.prepared_inputs.clone(),
            };
            match mutator.run_create_attempt(retry, self).await {
                Ok(value) => Ok(Box::new(value) as storage::corekv::RetryAttemptValue),
                Err(error) => Err(storage::corekv::RetryAttemptError::Abort(Box::new(error))),
            }
        })
    }
}

struct CreateManyRetryAttempt<S: Store> {
    db: Arc<DB<S>>,
    collection_name: String,
    collection: Collection,
    docs: Vec<Document>,
    identities: Vec<DocRef>,
    computed: Vec<ComputedBlocks>,
    context: RequestContext,
    acp_registration: Option<AcpRegistration>,
    prepared_inputs: Option<query::prepared::PreparedInputMemo>,
}

pub(super) fn acp_registration_retry_callback(
    acp: Arc<dyn acp::DocumentACP>,
    creator: identity::Did,
    policy_id: String,
    resource_name: String,
    doc_id: String,
    bearer_token: Option<String>,
) -> storage::corekv::CommitHook {
    Box::new(move || {
        Box::pin(async move {
            acp::with_request_bearer(
                &acp,
                Some(creator.to_string()),
                bearer_token,
                |acp| async move {
                    acp.redrive_registration(&policy_id, &resource_name, &doc_id)
                        .await
                        .map_err(|error| error.to_string())
                },
            )
            .await
        })
    })
}

impl<S: Store + 'static> storage::corekv::RetryAttemptFn for CreateManyRetryAttempt<S> {
    fn run<'a>(
        &'a mut self,
        retry: &'a mut dyn storage::corekv::RetryTxn,
    ) -> storage::corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            let mutator = AutoCommitMutator {
                db: Arc::clone(&self.db),
                prepared_inputs: self.prepared_inputs.clone(),
            };
            match mutator.run_create_many_attempt(retry, self).await {
                Ok(value) => Ok(Box::new(value) as storage::corekv::RetryAttemptValue),
                Err(error) => Err(storage::corekv::RetryAttemptError::Abort(Box::new(error))),
            }
        })
    }
}

#[allow(clippy::type_complexity)]
impl<S: Store + 'static> AutoCommitMutator<S> {
    /// The registration an external ACP is owed for a create by the request's
    /// creator, settled once the create commits.
    fn owed_acp_registration(
        &self,
        context: &RequestContext,
        collection: &Collection,
    ) -> query::error::Result<Option<AcpRegistration>> {
        match (
            collection.schema().policy.as_ref(),
            context.creator(),
            self.db.document_acp().cloned(),
        ) {
            (Some(policy), Some(creator), Some(acp)) if !acp.supports_transactions() => {
                let creator = identity::Did::new(creator).map_err(|error| {
                    query::error::QueryError::execution(format!(
                        "invalid mutation creator DID: {error}"
                    ))
                })?;
                let request_bearer_token = context.bearer_token_for(creator.as_str());
                Ok(Some(AcpRegistration {
                    acp,
                    creator,
                    policy_id: policy.id.clone(),
                    resource_name: policy.resource_name.clone(),
                    bearer_token: request_bearer_token,
                }))
            }
            _ => Ok(None),
        }
    }

    pub(super) async fn create_with_resolved_inputs(
        &self,
        context: &RequestContext,
        collection_name: &str,
        doc: Document,
    ) -> query::error::Result<CreateResult> {
        if self.prepared_inputs.is_some() {
            return self.create_impl(context, collection_name, doc).await;
        }
        let inputs = query::prepared::PreparedInputMemo::default();
        let mutator = Self {
            db: self.db.clone(),
            prepared_inputs: Some(inputs.clone()),
        };
        let mut doc = doc;
        mutator
            .seed_create_identity(collection_name, &mut doc)
            .await?;
        loop {
            match mutator
                .create_impl(context, collection_name, doc.clone())
                .await
            {
                Err(query::error::QueryError::NeedsInput(request)) => {
                    inputs
                        .answer_wave(&request, |wave| {
                            self.resolve_prepared_inputs_as(context, wave)
                        })
                        .await?;
                }
                result => return result,
            }
        }
    }

    pub(super) async fn create_many_with_resolved_inputs(
        &self,
        context: &RequestContext,
        collection_name: &str,
        docs: Vec<Document>,
    ) -> query::error::Result<Vec<CreateResult>> {
        if self.prepared_inputs.is_some() {
            return self.create_many_impl(context, collection_name, docs).await;
        }
        let inputs = query::prepared::PreparedInputMemo::default();
        let mutator = Self {
            db: self.db.clone(),
            prepared_inputs: Some(inputs.clone()),
        };
        let mut docs = docs;
        for doc in &mut docs {
            mutator.seed_create_identity(collection_name, doc).await?;
        }
        loop {
            match mutator
                .create_many_impl(context, collection_name, docs.clone())
                .await
            {
                Err(query::error::QueryError::NeedsInput(request)) => {
                    inputs
                        .answer_wave(&request, |wave| {
                            self.resolve_prepared_inputs_as(context, wave)
                        })
                        .await?;
                }
                result => return result,
            }
        }
    }

    async fn seed_create_identity(
        &self,
        collection_name: &str,
        doc: &mut Document,
    ) -> query::error::Result<()> {
        if crate::write::prepare::prepared_identity(doc).is_some() {
            return Ok(());
        }
        let (_collection_guard, collection) = self.guarded_collection(collection_name).await?;
        let identity = DocRef::new(
            collection.resolved_root_id(),
            self.db
                .next_doc_short_id()
                .await
                .map_err(|error| query::error::QueryError::execution(error.to_string()))?,
        );
        let mut preparation =
            document::WritePreparation::new(identity.collection_short_id, identity.doc_short_id);
        // The block builder uses this flag to distinguish a KMS-backed
        // missing key (which must become a prepared-input request) from the
        // legacy local-key path.
        preparation.set_requires_kms_keys(self.db.kms().is_some());
        doc.set_write_preparation(Arc::new(preparation));
        Ok(())
    }

    pub(super) async fn create_impl(
        &self,
        context: &RequestContext,
        collection_name: &str,
        mut doc: Document,
    ) -> query::error::Result<CreateResult> {
        self.db
            .check_node_access_as(context, acp::nac::NodePermission::DocumentUpdate)
            .await
            .map_err(|e| query::error::QueryError::permission_denied(e.to_string()))?;

        let (_collection_guard, collection) = self.guarded_collection(collection_name).await?;

        // Generate embeddings before blocks (embedding values affect the genesis CID)
        let embedding_config = self.db.options().embedding_config();
        if let Some(inputs) = &self.prepared_inputs {
            crate::search::embedding::set_embedding_from_inputs(
                &collection.schema().vector_embeddings,
                &mut doc,
                true,
                &rapidhash::RapidHashSet::default(),
                &embedding_config,
                inputs,
            )?;
        } else {
            crate::search::embedding::set_embedding_with(
                self.db.remote_inputs().embeddings(),
                &collection.schema().vector_embeddings,
                &mut doc,
                true,
                None,
                &embedding_config,
            )
            .await
            .map_err(|e| query::error::QueryError::execution(format!("embedding error: {}", e)))?;
        }

        let identity = match crate::write::prepare::prepared_identity(&doc) {
            Some(identity) => identity,
            None => DocRef::new(
                collection.resolved_root_id(),
                self.db
                    .next_doc_short_id()
                    .await
                    .map_err(|e| query::error::QueryError::execution(e.to_string()))?,
            ),
        };
        let doc_short_id = identity.doc_short_id;
        if crate::write::prepare::prepared_identity(&doc).is_none() {
            crate::write::prepare::prepare_keys(
                &self.db,
                &mut doc,
                identity,
                None,
                context.encryption(),
            )
            .await?;
        }

        let collection_version_id = collection.version_id().to_string();
        let acp_registration = self.owed_acp_registration(context, &collection)?;
        let computed_blocks = doc
            .write_preparation()
            .and_then(|write| write.payload::<PreparedCreateBlocks>())
            .filter(|prepared| prepared.collection_version_id == collection_version_id)
            .map(|prepared| prepared.computed.clone())
            .map(Ok)
            .unwrap_or_else(|| {
                compute_document_blocks_with_inputs(
                    &doc,
                    collection.schema(),
                    identity,
                    context.encryption(),
                    context.signing(),
                    self.prepared_inputs.as_ref(),
                )
            })
            .map_err(|error| error.into_query_error_in("failed to compute document blocks"))?;

        let doc = Arc::new(doc);

        // No per-doc write guard for creates: the DocID is derived from the
        // genesis block inside the txn, so no identity exists to guard yet.
        // The DocID-mapping duplicate check is the gate.
        let attempt = CreateRetryAttempt {
            db: Arc::clone(&self.db),
            collection_name: collection_name.to_string(),
            collection: collection.clone(),
            doc: Arc::clone(&doc),
            computed_blocks,
            doc_short_id,
            context: context.clone(),
            acp_registration,
            prepared_inputs: Some(self.prepared_inputs.clone().unwrap_or_default()),
        };
        let result = self
            .db
            .store()
            .transact_retry(self.db.options().retry_options(), Box::new(attempt))
            .await;
        let (doc_id, cid, block, col_data) = match result {
            Ok(value) => *value
                .downcast::<(DocID, Cid, Bytes, Option<(Cid, Bytes)>)>()
                .map_err(|_| query::error::QueryError::execution("invalid create retry result"))?,
            Err(error) => match error.downcast_abort::<query::error::QueryError>() {
                Ok(error) => return Err(error),
                Err(error) => {
                    return Err(crate::error::commit_query_error(
                        crate::error::Error::Storage(error.into_storage()),
                    ));
                }
            },
        };

        let mut doc = Arc::try_unwrap(doc).unwrap_or_else(|doc| doc.as_ref().clone());
        doc.set_id(doc_id.clone());
        doc.clear_write_preparation();
        let mut result = CreateResult::with_commit(doc_id, doc, cid, block);
        if let Some((col_cid, col_bytes)) = col_data {
            result.broadcast_cid = Some(col_cid);
            result.broadcast_block = Some(col_bytes);
        }
        Ok(result)
    }

    async fn run_create_attempt(
        &self,
        retry: &mut dyn storage::corekv::RetryTxn,
        attempt: &CreateRetryAttempt<S>,
    ) -> query::error::Result<(DocID, Cid, Bytes, Option<(Cid, Bytes)>)> {
        let io = retry
            .begin_attempt_io()
            .map_err(|error| crate::error::commit_query_error(error.into()))?;
        let basic_txn = datastore::BasicTxn::from_retry_attempt_io(io, 0);
        let txn = self.db.new_db_txn(basic_txn);
        let collection = txn
            .load_collection_for_write_attempt(&attempt.collection_name, &attempt.collection)
            .await
            .map_err(crate::error::commit_query_error)?;
        self.db
            .check_node_access_in_txn(
                &attempt.context,
                txn.acpstore().map_err(crate::error::commit_query_error)?,
                acp::nac::NodePermission::DocumentUpdate,
            )
            .await
            .map_err(crate::error::commit_query_error)?;

        let datastore = txn.datastore().map_err(|error| {
            query::error::QueryError::execution(format!("failed to get datastore: {error}"))
        })?;
        let systemstore = txn.systemstore().map_err(|error| {
            query::error::QueryError::execution(format!("failed to get systemstore: {error}"))
        })?;
        let blockstore = txn.blockstore().map_err(|error| {
            query::error::QueryError::execution(format!("failed to get blockstore: {error}"))
        })?;
        let headstore = txn.headstore().map_err(|error| {
            query::error::QueryError::execution(format!("failed to get headstore: {error}"))
        })?;

        let doc = attempt.doc.as_ref();
        let result: query::error::Result<(DocID, Cid, Bytes, Option<(Cid, Bytes)>)> = async {
            let index_manager = IndexManager::from_indexes(
                collection.resolved_root_id(),
                collection.schema(),
                collection.write_indexes(),
            )
            .map_err(|error| {
                query::error::QueryError::execution(format!(
                    "failed to create index manager for collection '{}': {error}",
                    attempt.collection_name
                ))
            })?;

            self.db
                .validate_downsample_write(&datastore, &systemstore, &collection, doc, None)
                .await
                .map_err(|error| query::error::QueryError::execution(error.to_string()))?;

            insert_computed_blocks(
                &blockstore,
                &headstore,
                &attempt.computed_blocks,
                attempt.context.batch_session(),
            )
            .await
            .map_err(|error| {
                query::error::QueryError::execution(format!(
                    "failed to write document blocks for create on collection {}: {error}",
                    attempt.collection_name
                ))
            })?;
            let block_result = &attempt.computed_blocks.block_result;
            let doc_id = register_created_doc(
                self.db.document_acp(),
                attempt.context.creator(),
                &systemstore,
                &datastore,
                &collection,
                attempt.doc_short_id,
                block_result,
            )
            .await?;
            // Keep the source document ID unset through validation above. The
            // derived ID is needed by series/index persistence, so clone and
            // annotate only after validation succeeds.
            let mut stored_doc = doc.clone();
            stored_doc.set_id(doc_id.clone());
            write_local_create(
                &datastore,
                &collection,
                &stored_doc,
                attempt.doc_short_id,
                &index_manager,
            )
            .await?;

            let col_block_data = write_branchable_collection_block(
                &self.db,
                &attempt.collection_name,
                &collection,
                &blockstore,
                &headstore,
                block_result.cid,
                attempt.doc_short_id,
                &attempt.context,
                attempt.prepared_inputs.as_ref(),
            )
            .await?;
            Ok((
                doc_id,
                block_result.cid,
                block_result.block.clone(),
                col_block_data,
            ))
        }
        .await;

        drop(datastore);
        drop(systemstore);
        drop(blockstore);
        drop(headstore);
        let (doc_id, cid, block, col_data) = match result {
            Ok(result) => result,
            Err(error) => {
                if let Err(discard_error) = txn.discard() {
                    warn!(error = %discard_error, "Failed to discard retry transaction after create error");
                }
                return Err(error);
            }
        };
        if let Some(registration) = attempt.acp_registration.as_ref() {
            txn.on_commit(OnCommit::OrderedAsync(acp_registration_retry_callback(
                Arc::clone(&registration.acp),
                registration.creator.clone(),
                registration.policy_id.clone(),
                registration.resource_name.clone(),
                doc_id.to_string(),
                registration.bearer_token.clone(),
            )))
            .map_err(crate::error::commit_query_error)?;
        }
        if let Some(bus) = self.db.event_bus().cloned() {
            let collection = collection.clone();
            let doc_id = doc_id.to_string();
            let block = block.clone();
            let col_data = col_data.clone();
            txn.on_commit(OnCommit::Ordered(Box::new(move || {
                let update = Update::new(
                    doc_id.clone(),
                    cid,
                    collection.collection_id().to_string(),
                    block,
                    false,
                    false,
                );
                bus.publish(Message::update(update));
                if let Some((col_cid, col_block)) = col_data {
                    let col_update = Update::new_with_subject_doc_id(
                        String::new(),
                        doc_id,
                        col_cid,
                        collection.collection_id().to_string(),
                        col_block,
                        false,
                        false,
                    );
                    bus.publish(Message::update(col_update));
                }
            })))
            .map_err(crate::error::commit_query_error)?;
        }
        let (io, callbacks) = txn
            .into_retry_attempt()
            .map_err(|error| crate::error::commit_query_error(error.into()))?;
        retry
            .finish_attempt_io(io, callbacks)
            .map_err(|error| crate::error::commit_query_error(error.into()))?;
        Ok((doc_id, cid, block, col_data))
    }

    pub(super) async fn create_many_impl(
        &self,
        context: &RequestContext,
        collection_name: &str,
        docs: Vec<Document>,
    ) -> query::error::Result<Vec<CreateResult>> {
        if docs.is_empty() {
            return Ok(Vec::new());
        }

        // Single-doc fast path: reuse existing create() to avoid overhead
        if docs.len() == 1 {
            let doc = docs.into_iter().next().expect("length checked above");
            return self
                .create_impl(context, collection_name, doc)
                .await
                .map(|r| vec![r]);
        }

        self.db
            .check_node_access_as(context, acp::nac::NodePermission::DocumentUpdate)
            .await
            .map_err(|e| query::error::QueryError::permission_denied(e.to_string()))?;

        let (_collection_guard, collection) = self.guarded_collection(collection_name).await?;

        let short_id = collection.resolved_root_id();
        let schema = collection.shared_schema();

        // === Phase 1: Prepare documents (sequential — embeddings are async/external) ===
        let mut prepared_docs: Vec<Document> = Vec::with_capacity(docs.len());
        let embedding_config = self.db.options().embedding_config();
        // Every document's missing embeddings join one wave.
        let mut first_miss = None;
        for mut doc in docs {
            if let Some(inputs) = &self.prepared_inputs {
                match crate::search::embedding::set_embedding_from_inputs(
                    &collection.schema().vector_embeddings,
                    &mut doc,
                    true,
                    &rapidhash::RapidHashSet::default(),
                    &embedding_config,
                    inputs,
                ) {
                    Ok(_) => {}
                    Err(query::error::QueryError::NeedsInput(request)) => {
                        first_miss.get_or_insert(request);
                    }
                    Err(error) => return Err(error),
                }
            } else {
                crate::search::embedding::set_embedding_with(
                    self.db.remote_inputs().embeddings(),
                    &collection.schema().vector_embeddings,
                    &mut doc,
                    true,
                    None,
                    &embedding_config,
                )
                .await
                .map_err(|e| {
                    query::error::QueryError::execution(format!("embedding error: {}", e))
                })?;
            }

            prepared_docs.push(doc);
        }
        if let Some(request) = first_miss {
            return Err(query::error::QueryError::NeedsInput(request));
        }

        // No per-doc write guards for creates: identities are derived inside
        // the txn; the DocID-mapping duplicate check is the gate.

        // === Phase 2: Prepare identities, keys, and deterministic blocks ===

        let mut identities = Vec::with_capacity(prepared_docs.len());
        for doc in &mut prepared_docs {
            let identity = match crate::write::prepare::prepared_identity(doc) {
                Some(identity) => identity,
                None => DocRef::new(
                    short_id,
                    self.db
                        .next_doc_short_id()
                        .await
                        .map_err(|e| query::error::QueryError::execution(e.to_string()))?,
                ),
            };
            if crate::write::prepare::prepared_identity(doc).is_none() {
                crate::write::prepare::prepare_keys(
                    &self.db,
                    doc,
                    identity,
                    None,
                    context.encryption(),
                )
                .await?;
            }
            identities.push(identity);
        }

        // Compute blocks (spread over a pool's workers and this task on
        // native, sequential on WASM): the database's pool, else the
        // process's default pool; on a worker before the default pool has
        // started (starting it would block the worker), all here. A failure
        // aborts the batch: without blocks there is no derived DocID.
        #[cfg(feature = "native")]
        let (prepared_docs, computed_blocks): (Vec<Document>, Vec<ComputedBlocks>) = {
            let pool = match self.db.pool_or_default() {
                Ok(pool) => Some(pool),
                Err(workers::Error::OnWorker) => None,
                Err(error) => {
                    return Err(query::error::QueryError::execution(format!(
                        "no worker pool can compute the document blocks: {error}"
                    )));
                }
            };
            let count = prepared_docs.len();
            let computed = runs::compute_blocks(
                pool.as_ref(),
                prepared_docs
                    .into_iter()
                    .zip(identities.iter().copied())
                    .collect(),
                &schema,
                context,
                self.prepared_inputs.as_ref(),
            )
            .await?;
            let mut docs = Vec::with_capacity(count);
            let mut blocks = Vec::with_capacity(count);
            for (doc, computed) in computed {
                match computed {
                    Ok(computed) => blocks.push(computed),
                    Err(crate::block::builder::BuildError::NeedsInput(request)) => {
                        return Err(query::error::QueryError::NeedsInput(request));
                    }
                    Err(error) => {
                        return Err(query::error::QueryError::execution(format!(
                            "failed to compute document blocks: {error}"
                        )));
                    }
                }
                docs.push(doc);
            }
            (docs, blocks)
        };

        #[cfg(not(feature = "native"))]
        let computed_blocks: Vec<ComputedBlocks> = {
            let mut computed = Vec::with_capacity(prepared_docs.len());
            // Every document's missing inputs join one wave: a miss is kept
            // and the next document computed.
            let mut first_miss = None;
            for (doc, identity) in prepared_docs.iter().zip(identities.iter()) {
                match compute_document_blocks_with_inputs(
                    doc,
                    &schema,
                    *identity,
                    context.encryption(),
                    context.signing(),
                    self.prepared_inputs.as_ref(),
                ) {
                    Ok(blocks) => computed.push(blocks),
                    Err(crate::block::builder::BuildError::NeedsInput(request)) => {
                        first_miss.get_or_insert(request);
                    }
                    Err(error) => {
                        return Err(error.into_query_error_in("failed to compute document blocks"))
                    }
                }
            }
            if let Some(request) = first_miss {
                return Err(query::error::QueryError::NeedsInput(request));
            }
            computed
        };

        let acp_registration = self.owed_acp_registration(context, &collection)?;
        let attempt = CreateManyRetryAttempt {
            db: Arc::clone(&self.db),
            collection_name: collection_name.to_string(),
            collection: collection.clone(),
            docs: prepared_docs,
            identities,
            computed: computed_blocks,
            context: context.clone(),
            acp_registration,
            prepared_inputs: Some(self.prepared_inputs.clone().unwrap_or_default()),
        };
        let result = self
            .db
            .store()
            .transact_retry(self.db.options().retry_options(), Box::new(attempt))
            .await;
        let results = match result {
            Ok(value) => *value
                .downcast::<Vec<(DocID, Document, Cid, Bytes, Option<(Cid, Bytes)>)>>()
                .map_err(|_| {
                    query::error::QueryError::execution("invalid batch create retry result")
                })?,
            Err(error) => match error.downcast_abort::<query::error::QueryError>() {
                Ok(error) => return Err(error),
                Err(error) => {
                    return Err(crate::error::commit_query_error(
                        crate::error::Error::Storage(error.into_storage()),
                    ));
                }
            },
        };
        let mut create_results = Vec::with_capacity(results.len());
        for (doc_id, mut doc, cid, block, col_data) in results {
            doc.clear_write_preparation();
            let mut result = CreateResult::with_commit(doc_id, doc, cid, block);
            if let Some((col_cid, col_bytes)) = col_data {
                result.broadcast_cid = Some(col_cid);
                result.broadcast_block = Some(col_bytes);
            }
            create_results.push(result);
        }
        Ok(create_results)
    }

    async fn run_create_many_attempt(
        &self,
        retry: &mut dyn storage::corekv::RetryTxn,
        attempt: &CreateManyRetryAttempt<S>,
    ) -> query::error::Result<Vec<(DocID, Document, Cid, Bytes, Option<(Cid, Bytes)>)>> {
        let io = retry
            .begin_attempt_io()
            .map_err(|error| crate::error::commit_query_error(error.into()))?;
        let basic_txn = datastore::BasicTxn::from_retry_attempt_io(io, 0);
        let txn = self.db.new_db_txn(basic_txn);
        let collection = txn
            .load_collection_for_write_attempt(&attempt.collection_name, &attempt.collection)
            .await
            .map_err(crate::error::commit_query_error)?;
        self.db
            .check_node_access_in_txn(
                &attempt.context,
                txn.acpstore().map_err(crate::error::commit_query_error)?,
                acp::nac::NodePermission::DocumentUpdate,
            )
            .await
            .map_err(crate::error::commit_query_error)?;
        let datastore = txn.datastore().map_err(|error| {
            query::error::QueryError::execution(format!("failed to get datastore: {error}"))
        })?;
        let systemstore = txn.systemstore().map_err(|error| {
            query::error::QueryError::execution(format!("failed to get systemstore: {error}"))
        })?;
        let blockstore = txn.blockstore().map_err(|error| {
            query::error::QueryError::execution(format!("failed to get blockstore: {error}"))
        })?;
        let headstore = txn.headstore().map_err(|error| {
            query::error::QueryError::execution(format!("failed to get headstore: {error}"))
        })?;
        let result: query::error::Result<Vec<(DocID, Document, Cid, Bytes, Option<(Cid, Bytes)>)>> =
            async {
                let index_manager = IndexManager::from_indexes(
                    collection.resolved_root_id(),
                    collection.schema(),
                    collection.write_indexes(),
                )
                .map_err(|error| {
                    query::error::QueryError::execution(format!(
                        "failed to create index manager: {error}"
                    ))
                })?;
                let mut results = Vec::with_capacity(attempt.docs.len());
                for ((mut doc, identity), computed) in attempt
                    .docs
                    .iter()
                    .cloned()
                    .zip(attempt.identities.iter().copied())
                    .zip(&attempt.computed)
                {
                    self.db
                        .validate_downsample_write(
                            &datastore,
                            &systemstore,
                            &collection,
                            &doc,
                            None,
                        )
                        .await
                        .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
                    insert_computed_blocks(
                        &blockstore,
                        &headstore,
                        computed,
                        attempt.context.batch_session(),
                    )
                    .await
                    .map_err(|error| {
                        query::error::QueryError::execution(format!(
                            "failed to insert precomputed blocks: {error}"
                        ))
                    })?;
                    let doc_id = register_created_doc(
                        self.db.document_acp(),
                        attempt.context.creator(),
                        &systemstore,
                        &datastore,
                        &collection,
                        identity.doc_short_id,
                        &computed.block_result,
                    )
                    .await?;
                    doc.set_id(doc_id.clone());
                    write_local_create(
                        &datastore,
                        &collection,
                        &doc,
                        identity.doc_short_id,
                        &index_manager,
                    )
                    .await?;
                    let col_data = write_branchable_collection_block(
                        &self.db,
                        &attempt.collection_name,
                        &collection,
                        &blockstore,
                        &headstore,
                        computed.block_result.cid,
                        identity.doc_short_id,
                        &attempt.context,
                        attempt.prepared_inputs.as_ref(),
                    )
                    .await?;
                    results.push((
                        doc_id,
                        doc,
                        computed.block_result.cid,
                        computed.block_result.block.clone(),
                        col_data,
                    ));
                }
                Ok(results)
            }
            .await;
        drop(datastore);
        drop(systemstore);
        drop(blockstore);
        drop(headstore);
        let results = match result {
            Ok(results) => results,
            Err(error) => {
                if let Err(discard_error) = txn.discard() {
                    warn!(error = %discard_error, "Failed to discard retry transaction after batch create error");
                }
                return Err(error);
            }
        };
        for (doc_id, _, cid, block, col_data) in &results {
            if let Some(registration) = attempt.acp_registration.as_ref() {
                txn.on_commit(OnCommit::OrderedAsync(acp_registration_retry_callback(
                    Arc::clone(&registration.acp),
                    registration.creator.clone(),
                    registration.policy_id.clone(),
                    registration.resource_name.clone(),
                    doc_id.to_string(),
                    registration.bearer_token.clone(),
                )))
                .map_err(crate::error::commit_query_error)?;
            }
            if let Some(bus) = self.db.event_bus().cloned() {
                let collection = collection.clone();
                let doc_id = doc_id.to_string();
                let cid = *cid;
                let block = block.clone();
                let col_data = col_data.clone();
                txn.on_commit(OnCommit::Ordered(Box::new(move || {
                    bus.publish(Message::update(Update::new(
                        doc_id.clone(),
                        cid,
                        collection.collection_id().to_string(),
                        block,
                        false,
                        false,
                    )));
                    if let Some((col_cid, col_block)) = col_data {
                        bus.publish(Message::update(Update::new_with_subject_doc_id(
                            String::new(),
                            doc_id,
                            col_cid,
                            collection.collection_id().to_string(),
                            col_block,
                            false,
                            false,
                        )));
                    }
                })))
                .map_err(crate::error::commit_query_error)?;
            }
        }
        let (io, callbacks) = txn
            .into_retry_attempt()
            .map_err(|error| crate::error::commit_query_error(error.into()))?;
        retry
            .finish_attempt_io(io, callbacks)
            .map_err(|error| crate::error::commit_query_error(error.into()))?;
        Ok(results)
    }
}
