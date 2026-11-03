use super::helpers::write_branchable_collection_block;
use super::*;
use events::{Message, Update};
use storage::corekv::OnCommit;

type DeleteAttemptResult = (DocID, bool, Option<CommitArtifacts>);

struct DeleteRetryAttempt<S: Store> {
    db: Arc<DB<S>>,
    collection_name: String,
    collection: Collection,
    doc_id: DocID,
    context: RequestContext,
    prepared_inputs: Option<query::prepared::PreparedInputMemo>,
    heads: super::base_heads::BaseHeads,
}

struct DeleteManyRetryAttempt<S: Store> {
    db: Arc<DB<S>>,
    collection_name: String,
    collection: Collection,
    doc_ids: Vec<DocID>,
    context: RequestContext,
    prepared_inputs: Option<query::prepared::PreparedInputMemo>,
    heads: super::base_heads::BaseHeads,
}

impl<S: Store + 'static> storage::corekv::RetryAttemptFn for DeleteManyRetryAttempt<S> {
    fn run<'a>(
        &'a mut self,
        retry: &'a mut dyn storage::corekv::RetryTxn,
    ) -> storage::corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            let mutator = AutoCommitMutator {
                db: Arc::clone(&self.db),
                prepared_inputs: self.prepared_inputs.clone(),
            };
            match mutator.run_delete_many_attempt(retry, self).await {
                Ok(value) => Ok(Box::new(value) as storage::corekv::RetryAttemptValue),
                Err(error) => Err(storage::corekv::RetryAttemptError::Abort(Box::new(error))),
            }
        })
    }
}

impl<S: Store + 'static> storage::corekv::RetryAttemptFn for DeleteRetryAttempt<S> {
    fn run<'a>(
        &'a mut self,
        retry: &'a mut dyn storage::corekv::RetryTxn,
    ) -> storage::corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            let mutator = AutoCommitMutator {
                db: Arc::clone(&self.db),
                prepared_inputs: self.prepared_inputs.clone(),
            };
            match mutator.run_delete_attempt(retry, self).await {
                Ok(value) => Ok(Box::new(value) as storage::corekv::RetryAttemptValue),
                Err(error) => Err(storage::corekv::RetryAttemptError::Abort(Box::new(error))),
            }
        })
    }
}

impl<S: Store + 'static> AutoCommitMutator<S> {
    pub(super) async fn delete_with_resolved_inputs(
        &self,
        context: &RequestContext,
        collection_name: &str,
        doc_id: &DocID,
    ) -> query::error::Result<DeleteResult> {
        if self.prepared_inputs.is_some() {
            return self.delete_impl(context, collection_name, doc_id).await;
        }
        let inputs = query::prepared::PreparedInputMemo::default();
        let mutator = Self {
            db: self.db.clone(),
            prepared_inputs: Some(inputs.clone()),
        };
        loop {
            match mutator.delete_impl(context, collection_name, doc_id).await {
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

    /// [`Self::delete_many_impl_as`] in [`RequestContext::ambient`].
    pub async fn delete_many_impl(
        &self,
        collection_name: &str,
        doc_ids: &[DocID],
    ) -> query::error::Result<Vec<DeleteResult>> {
        self.delete_many_impl_as(&RequestContext::ambient(), collection_name, doc_ids)
            .await
    }

    pub async fn delete_many_impl_as(
        &self,
        context: &RequestContext,
        collection_name: &str,
        doc_ids: &[DocID],
    ) -> query::error::Result<Vec<DeleteResult>> {
        if self.prepared_inputs.is_some() {
            return self
                .delete_many_once_impl(context, collection_name, doc_ids)
                .await;
        }
        let inputs = query::prepared::PreparedInputMemo::default();
        let mutator = Self {
            db: self.db.clone(),
            prepared_inputs: Some(inputs.clone()),
        };
        loop {
            match mutator
                .delete_many_once_impl(context, collection_name, doc_ids)
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

    pub(super) async fn delete_impl(
        &self,
        context: &RequestContext,
        collection_name: &str,
        doc_id: &DocID,
    ) -> query::error::Result<DeleteResult> {
        self.db
            .check_node_access_as(context, acp::nac::NodePermission::DocumentDelete)
            .await
            .map_err(|e| query::error::QueryError::permission_denied(e.to_string()))?;

        let (_collection_guard, collection) = self.guarded_collection(collection_name).await?;

        let heads = super::base_heads::BaseHeads::read_if_signed(
            &self.db,
            &collection,
            std::slice::from_ref(doc_id),
            context,
        )
        .await?;
        let attempt = DeleteRetryAttempt {
            db: Arc::clone(&self.db),
            collection_name: collection_name.to_string(),
            collection: collection.clone(),
            doc_id: doc_id.clone(),
            context: context.clone(),
            prepared_inputs: Some(self.prepared_inputs.clone().unwrap_or_default()),
            heads,
        };
        let result = self
            .db
            .store()
            .transact_retry(self.db.options().retry_options(), Box::new(attempt))
            .await;
        let (canonical_doc_id, commit_result) = match result {
            Ok(value) => *value
                .downcast::<(DocID, Option<CommitArtifacts>)>()
                .map_err(|_| query::error::QueryError::execution("invalid delete retry result"))?,
            Err(error) => match error.downcast_abort::<query::error::QueryError>() {
                Ok(error) => return Err(error),
                Err(error) => {
                    return Err(crate::error::commit_query_error(
                        crate::error::Error::Storage(error.into_storage()),
                    ));
                }
            },
        };
        let Some((cid, block, col_data)) = commit_result else {
            return Ok(DeleteResult::new(doc_id.clone(), false));
        };
        let mut result = DeleteResult::with_commit(canonical_doc_id, true, cid, block);
        if let Some((col_cid, col_bytes)) = col_data {
            result.broadcast_cid = Some(col_cid);
            result.broadcast_block = Some(col_bytes);
        }
        Ok(result)
    }

    async fn run_delete_attempt(
        &self,
        retry: &mut dyn storage::corekv::RetryTxn,
        attempt: &DeleteRetryAttempt<S>,
    ) -> query::error::Result<(DocID, Option<CommitArtifacts>)> {
        let io = retry
            .begin_attempt_io()
            .map_err(|error| crate::error::commit_query_error(error.into()))?;
        let basic_txn = datastore::BasicTxn::from_retry_attempt_io(io, 0, self.db.crdts().clone());
        let txn = self.db.new_db_txn(basic_txn);
        let collection = txn
            .load_collection_for_write_attempt(&attempt.collection_name, &attempt.collection)
            .await
            .map_err(crate::error::commit_query_error)?;

        self.db
            .check_node_access_in_txn(
                &attempt.context,
                txn.acpstore().map_err(crate::error::commit_query_error)?,
                acp::nac::NodePermission::DocumentDelete,
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
        let result: query::error::Result<(DocID, Option<CommitArtifacts>)> = async {
            let Some((doc_short_id, canonical_doc_id)) = collection
                .resolve_doc_identity(&systemstore, &attempt.doc_id)
                .await
                .map_err(|error| query::error::QueryError::execution(error.to_string()))?
            else {
                return Ok((attempt.doc_id.clone(), None));
            };
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
            let index_manager = index_manager.with_vector_caches(self.db.vector_codes());
            let existed = collection
                .delete_with_indexes(&datastore, &canonical_doc_id, doc_short_id, &index_manager)
                .await
                .map_err(|error| {
                    query::error::QueryError::execution(format!("delete error: {error}"))
                })?;
            if !existed {
                return Ok((canonical_doc_id, None));
            }
            crate::write::autocommit::helpers::unregister_deleted_doc(
                self.db.document_acp(),
                &systemstore,
                &collection,
                &canonical_doc_id,
            )
            .await?;
            let doc_id_string = canonical_doc_id.to_string();
            let heads = attempt.heads.for_attempt(&headstore, doc_short_id).await?;
            let block_result = write_delete_block_with_inputs(
                &blockstore,
                &headstore,
                &doc_id_string,
                doc_short_id,
                collection.version_id(),
                attempt.context.signing(),
                attempt.context.batch_session(),
                attempt.prepared_inputs.as_ref(),
                Some(heads.into_owned()),
            )
            .await
            .map_err(|error| error.into_query_error_in("failed to write delete block"))?;
            crate::docid::map::set_block_doc_id_mapping(
                &systemstore,
                &block_result.cid.to_string(),
                &doc_id_string,
            )
            .await
            .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
            let col_data = write_branchable_collection_block(
                &self.db,
                &attempt.collection_name,
                &collection,
                &blockstore,
                &headstore,
                block_result.cid,
                doc_short_id,
                &attempt.context,
                attempt.prepared_inputs.as_ref(),
            )
            .await?;
            Ok((
                canonical_doc_id,
                Some((block_result.cid, block_result.block, col_data)),
            ))
        }
        .await;
        drop(datastore);
        drop(systemstore);
        drop(blockstore);
        drop(headstore);
        let (canonical_doc_id, artifacts) = match result {
            Ok(result) => result,
            Err(error) => {
                if let Err(discard_error) = txn.discard() {
                    warn!(error = %discard_error, "Failed to discard retry transaction after delete error");
                }
                return Err(error);
            }
        };
        if let Some((cid, block, col_data)) = &artifacts {
            self.register_deleted_doc_with_acp_as(
                &attempt.context,
                &txn,
                &collection,
                canonical_doc_id.to_string(),
            )?;
            if let Some(bus) = self.db.event_bus().cloned() {
                let collection = collection.clone();
                let doc_id = canonical_doc_id.to_string();
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
        Ok((canonical_doc_id, artifacts))
    }

    /// Delete multiple documents in a single transaction.
    ///
    /// Mirrors `create_many_impl`: one transaction for all deletes, one commit.
    /// This avoids creating N separate MVCC snapshots and N COW epochs when
    /// purging large batches (e.g. 686 documents caused a 20GB memory spike
    /// with per-document transactions).
    async fn delete_many_once_impl(
        &self,
        context: &RequestContext,
        collection_name: &str,
        doc_ids: &[DocID],
    ) -> query::error::Result<Vec<DeleteResult>> {
        if doc_ids.is_empty() {
            return Ok(Vec::new());
        }

        if doc_ids.len() == 1 {
            return self
                .delete_impl(context, collection_name, &doc_ids[0])
                .await
                .map(|r| vec![r]);
        }

        let (_collection_guard, collection) = self.guarded_collection(collection_name).await?;
        let heads =
            super::base_heads::BaseHeads::read_if_signed(&self.db, &collection, doc_ids, context)
                .await?;
        let attempt = DeleteManyRetryAttempt {
            db: Arc::clone(&self.db),
            collection_name: collection_name.to_string(),
            collection: collection.clone(),
            doc_ids: doc_ids.to_vec(),
            context: context.clone(),
            prepared_inputs: Some(self.prepared_inputs.clone().unwrap_or_default()),
            heads,
        };
        let result = self
            .db
            .store()
            .transact_retry(self.db.options().retry_options(), Box::new(attempt))
            .await;
        let results = match result {
            Ok(value) => *value.downcast::<Vec<DeleteAttemptResult>>().map_err(|_| {
                query::error::QueryError::execution("invalid batch delete retry result")
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
        Ok(results
            .into_iter()
            .map(|(doc_id, existed, _)| DeleteResult::new(doc_id, existed))
            .collect())
    }

    async fn run_delete_many_attempt(
        &self,
        retry: &mut dyn storage::corekv::RetryTxn,
        attempt: &DeleteManyRetryAttempt<S>,
    ) -> query::error::Result<Vec<DeleteAttemptResult>> {
        let io = retry
            .begin_attempt_io()
            .map_err(|error| crate::error::commit_query_error(error.into()))?;
        let basic_txn = datastore::BasicTxn::from_retry_attempt_io(io, 0, self.db.crdts().clone());
        let txn = self.db.new_db_txn(basic_txn);
        let collection = txn
            .load_collection_for_write_attempt(&attempt.collection_name, &attempt.collection)
            .await
            .map_err(crate::error::commit_query_error)?;

        self.db
            .check_node_access_in_txn(
                &attempt.context,
                txn.acpstore().map_err(crate::error::commit_query_error)?,
                acp::nac::NodePermission::DocumentDelete,
            )
            .await
            .map_err(crate::error::commit_query_error)?;
        let datastore = txn.datastore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get datastore: {e}"))
        })?;
        let systemstore = txn.systemstore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get systemstore: {e}"))
        })?;
        let blockstore = txn.blockstore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get blockstore: {e}"))
        })?;
        let headstore = txn.headstore().map_err(|e| {
            query::error::QueryError::execution(format!("failed to get headstore: {e}"))
        })?;
        let result: query::error::Result<Vec<DeleteAttemptResult>> = async {
            let index_manager = IndexManager::from_indexes(
                collection.resolved_root_id(),
                collection.schema(),
                collection.write_indexes(),
            )
            .map_err(|e| {
                query::error::QueryError::execution(format!("failed to create index manager: {e}"))
            })?;
            let index_manager = index_manager.with_vector_caches(self.db.vector_codes());
            let mut results = Vec::with_capacity(attempt.doc_ids.len());
            for requested_id in &attempt.doc_ids {
                let Some((doc_short_id, canonical_id)) = collection
                    .resolve_doc_identity(&systemstore, requested_id)
                    .await
                    .map_err(|e| {
                        query::error::QueryError::execution(format!(
                            "failed to resolve document identity for '{requested_id}': {e}"
                        ))
                    })?
                else {
                    results.push((requested_id.clone(), false, None));
                    continue;
                };
                let existed = collection
                    .delete_with_indexes(&datastore, &canonical_id, doc_short_id, &index_manager)
                    .await
                    .map_err(|e| {
                        query::error::QueryError::execution(format!("delete error: {e}"))
                    })?;
                if !existed {
                    results.push((canonical_id, false, None));
                    continue;
                }
                crate::write::autocommit::helpers::unregister_deleted_doc(
                    self.db.document_acp(),
                    &systemstore,
                    &collection,
                    &canonical_id,
                )
                .await?;
                let heads = attempt.heads.for_attempt(&headstore, doc_short_id).await?;
                let block_result = write_delete_block_with_inputs(
                    &blockstore,
                    &headstore,
                    &canonical_id.to_string(),
                    doc_short_id,
                    collection.version_id(),
                    attempt.context.signing(),
                    attempt.context.batch_session(),
                    attempt.prepared_inputs.as_ref(),
                    Some(heads.into_owned()),
                )
                .await
                .map_err(|error| error.into_query_error_in("failed to write delete block"))?;
                crate::docid::map::set_block_doc_id_mapping(
                    &systemstore,
                    &block_result.cid.to_string(),
                    &canonical_id.to_string(),
                )
                .await
                .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
                let col_data = write_branchable_collection_block(
                    &self.db,
                    &attempt.collection_name,
                    &collection,
                    &blockstore,
                    &headstore,
                    block_result.cid,
                    doc_short_id,
                    &attempt.context,
                    attempt.prepared_inputs.as_ref(),
                )
                .await?;
                results.push((
                    canonical_id,
                    true,
                    Some((block_result.cid, block_result.block, col_data)),
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
                let _ = txn.discard();
                return Err(error);
            }
        };
        for (doc_id, _, artifacts) in &results {
            if artifacts.is_some() {
                self.register_deleted_doc_with_acp_as(
                    &attempt.context,
                    &txn,
                    &collection,
                    doc_id.to_string(),
                )?;
            }
        }
        if let Some(bus) = self.db.event_bus().cloned() {
            for (doc_id, _, artifacts) in &results {
                let Some((cid, block, col_data)) = artifacts else {
                    continue;
                };
                let bus = Arc::clone(&bus);
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
            .map_err(|e| crate::error::commit_query_error(e.into()))?;
        retry
            .finish_attempt_io(io, callbacks)
            .map_err(|e| crate::error::commit_query_error(e.into()))?;
        Ok(results)
    }
}
