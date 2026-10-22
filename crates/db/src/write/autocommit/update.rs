use super::helpers::{
    register_block_doc_id_mappings, write_branchable_collection_block, write_local_update,
};
use super::*;
use events::{Message, Update};
use storage::corekv::OnCommit;

use crate::block::builder::DocRef;
use query::runner::DocFetcher;

struct UpdateRetryAttempt<S: Store> {
    db: Arc<DB<S>>,
    collection_name: String,
    collection: Collection,
    expected: Option<Document>,
    expected_fields: Option<rapidhash::RapidHashSet<String>>,
    input: Document,
    modified_fields: rapidhash::RapidHashSet<String>,
    input_doc_id: DocID,
    canonical_doc_id: String,
    context: RequestContext,
    prepared_inputs: Option<query::prepared::PreparedInputMemo>,
    heads: super::base_heads::BaseHeads,
}

impl<S: Store + 'static> storage::corekv::RetryAttemptFn for UpdateRetryAttempt<S> {
    fn run<'a>(
        &'a mut self,
        retry: &'a mut dyn storage::corekv::RetryTxn,
    ) -> storage::corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            let mutator = AutoCommitMutator {
                db: Arc::clone(&self.db),
                prepared_inputs: self.prepared_inputs.clone(),
            };
            match mutator.run_update_attempt(retry, self).await {
                Ok(value) => Ok(Box::new(value) as storage::corekv::RetryAttemptValue),
                Err(error) => Err(storage::corekv::RetryAttemptError::Abort(Box::new(error))),
            }
        })
    }
}

#[allow(clippy::type_complexity)]
impl<S: Store + 'static> AutoCommitMutator<S> {
    pub(super) async fn update_with_resolved_inputs(
        &self,
        context: &RequestContext,
        collection_name: &str,
        expected: Option<Document>,
        doc: Document,
        fields: rapidhash::RapidHashSet<String>,
    ) -> query::error::Result<UpdateResult> {
        self.update_with_projected_resolved_inputs(
            context,
            collection_name,
            expected,
            None,
            doc,
            fields,
        )
        .await
    }

    pub(super) async fn update_with_projected_resolved_inputs(
        &self,
        context: &RequestContext,
        collection_name: &str,
        expected: Option<Document>,
        expected_fields: Option<rapidhash::RapidHashSet<String>>,
        doc: Document,
        fields: rapidhash::RapidHashSet<String>,
    ) -> query::error::Result<UpdateResult> {
        if self.prepared_inputs.is_some() {
            return self
                .update_impl(
                    context,
                    collection_name,
                    expected,
                    expected_fields,
                    doc,
                    fields,
                )
                .await;
        }
        let inputs = query::prepared::PreparedInputMemo::default();
        let mutator = Self {
            db: self.db.clone(),
            prepared_inputs: Some(inputs.clone()),
        };
        loop {
            match mutator
                .update_impl(
                    context,
                    collection_name,
                    expected.clone(),
                    expected_fields.clone(),
                    doc.clone(),
                    fields.clone(),
                )
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

    pub(super) async fn update_impl(
        &self,
        context: &RequestContext,
        collection_name: &str,
        expected: Option<Document>,
        expected_fields: Option<rapidhash::RapidHashSet<String>>,
        mut doc: Document,
        modified_fields: rapidhash::RapidHashSet<String>,
    ) -> query::error::Result<UpdateResult> {
        self.db
            .check_node_access_as(context, acp::nac::NodePermission::DocumentUpdate)
            .await
            .map_err(|error| query::error::QueryError::permission_denied(error.to_string()))?;

        let (_collection_guard, collection) = self.guarded_collection(collection_name).await?;
        let input_doc_id = doc
            .id()
            .cloned()
            .ok_or_else(|| query::error::QueryError::execution("update requires a document ID"))?;

        // Resolve the canonical ID and the heads the update builds on, at one
        // snapshot. The retry closure repeats the document read and
        // stale-input comparison against each fresh engine snapshot.
        let (canonical, heads) =
            super::base_heads::BaseHeads::read_identified(&self.db, &collection, &input_doc_id)
                .await?;
        let canonical_doc_id = canonical.to_string();
        doc.set_id(canonical);

        let attempt = UpdateRetryAttempt {
            db: Arc::clone(&self.db),
            collection_name: collection_name.to_string(),
            collection: collection.clone(),
            expected,
            expected_fields,
            input: doc,
            modified_fields,
            input_doc_id,
            canonical_doc_id,
            context: context.clone(),
            prepared_inputs: Some(self.prepared_inputs.clone().unwrap_or_default()),
            heads,
        };
        let result = self
            .db
            .store()
            .transact_retry(self.db.options().retry_options(), Box::new(attempt))
            .await;
        let (doc, fields_modified, commit) = match result {
            Ok(value) => *value
                .downcast::<UpdateAttemptOutput>()
                .map_err(|_| query::error::QueryError::execution("invalid update retry result"))?,
            Err(error) => match error.downcast_abort::<query::error::QueryError>() {
                Ok(error) => return Err(error),
                Err(error) => {
                    return Err(crate::error::commit_query_error(
                        crate::error::Error::Storage(error.into_storage()),
                    ));
                }
            },
        };

        let mut doc = doc;
        doc.clear_write_preparation();
        let Some((cid, block, col_data)) = commit else {
            return Ok(UpdateResult::new(doc, fields_modified));
        };
        let mut result = UpdateResult::with_commit(doc, fields_modified, cid, block);
        if let Some((col_cid, col_bytes)) = col_data {
            result.broadcast_cid = Some(col_cid);
            result.broadcast_block = Some(col_bytes);
        }
        Ok(result)
    }

    async fn run_update_attempt(
        &self,
        retry: &mut dyn storage::corekv::RetryTxn,
        attempt: &UpdateRetryAttempt<S>,
    ) -> query::error::Result<UpdateAttemptOutput> {
        // A compare-and-set that finds the document changed lost a race the
        // caller asked for: report it with the reason the engine gave.
        let lost = retry.previous_conflict();
        let lost_race = || {
            query::error::QueryError::transaction_conflict(
                lost.map_or(
                    storage::Error::TxnConflict,
                    storage::Error::TxnConflictDetails,
                )
                .to_string(),
            )
        };
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
                acp::nac::NodePermission::DocumentUpdate,
            )
            .await
            .map_err(crate::error::commit_query_error)?;

        // The fresh read, stale-plan comparison, rebase, and writes all belong
        // to this attempt. A storage conflict therefore repeats the complete
        // decision against the next engine snapshot.
        // The parts this update decides on: those its plan compared and those
        // its writes decide on in this run.
        let run = query::update_parts::Run::after(lost.as_ref());
        let mut dependencies = run.written(
            collection.crdts(),
            collection.schema(),
            collection.write_indexes(),
            &attempt.modified_fields,
            |field| attempt.input.pending_op(field).is_some(),
        );
        dependencies.extend(attempt.expected_fields.iter().flatten().cloned());
        let projected = attempt.expected.is_none() || attempt.expected_fields.is_some();
        let fetcher = if projected {
            LensedDocFetcher::new_projected(
                Arc::clone(&self.db),
                txn,
                self.db.lens_store.clone(),
                dependencies.clone(),
            )
        } else {
            LensedDocFetcher::new(Arc::clone(&self.db), txn, self.db.lens_store.clone())
        };
        let docs = fetcher
            .get_by_ids(
                &attempt.collection_name,
                std::slice::from_ref(&attempt.canonical_doc_id),
            )
            .await?;
        let Some(mut current_doc) = docs.into_docs().into_iter().next() else {
            return Err(if attempt.expected.is_some() {
                lost_race()
            } else {
                query::error::QueryError::document_not_found(attempt.canonical_doc_id.clone())
            });
        };
        if let Some(expected) = &attempt.expected {
            let unchanged = if attempt.expected_fields.is_some() {
                dependencies
                    .iter()
                    .all(|field| expected.get(field) == current_doc.get(field))
            } else {
                expected.values().len() == current_doc.values().len()
                    && expected
                        .values()
                        .iter()
                        .all(|(name, value)| current_doc.get(name) == Some(value.value()))
            };
            if expected.id() != current_doc.id()
                || expected.is_deleted() != current_doc.is_deleted()
                || !unchanged
            {
                return Err(lost_race());
            }
        }
        // A field that already holds the written value needs no block: its
        // LWW priority would only re-assert the current state. Counter inputs
        // are deltas, an explicit encryption config re-keys every field, and a
        // downsample source records every write as a sample. Only a field this
        // attempt read is compared: that read is validated, so a concurrent
        // change of the field conflicts and the comparison runs again.
        let mut modified_fields = attempt.modified_fields.clone();
        let downsample_source = self
            .db
            .is_downsample_source(&attempt.collection_name)
            .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
        let unchanged_skips = attempt.context.encryption().is_none() && !downsample_source;
        if unchanged_skips {
            modified_fields.retain(|field| {
                attempt.input.pending_op(field).is_some()
                    || (projected && !dependencies.contains(field))
                    || attempt.input.get(field) != current_doc.get(field)
            });
        }
        for field in &modified_fields {
            if let Some(op) = attempt.input.pending_op(field).cloned() {
                current_doc
                    .edit(field.clone(), op)
                    .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
            } else if let Some(value) = attempt.input.get(field).cloned() {
                current_doc
                    .set(field.clone(), value)
                    .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
            }
        }
        if let Some(prepared) = attempt.input.write_preparation().cloned() {
            current_doc.set_write_preparation(prepared);
        }
        let txn = fetcher.take_txn().await.ok_or_else(|| {
            query::error::QueryError::execution("update transaction already consumed")
        })?;
        drop(fetcher);
        if unchanged_skips && modified_fields.is_empty() {
            // Nothing to write; the attempt still commits its reads, so a
            // concurrent change of a compared field runs it again.
            let (io, callbacks) = txn
                .into_retry_attempt()
                .map_err(|error| crate::error::commit_query_error(error.into()))?;
            retry
                .finish_attempt_io(io, callbacks)
                .map_err(|error| crate::error::commit_query_error(error.into()))?;
            return Ok((current_doc, 0, None));
        }

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

        let (doc_short_id, canonical_doc_id) = collection
            .require_doc_identity(&systemstore, &attempt.input_doc_id)
            .await
            .map_err(|error| match error {
                crate::error::Error::DocumentNotFound(id) => {
                    query::error::QueryError::document_not_found(id)
                }
                other => query::error::QueryError::execution(other.to_string()),
            })?;
        current_doc.set_id(canonical_doc_id.clone());
        let embedding_config = self.db.options().embedding_config();
        let inputs = attempt.prepared_inputs.clone().unwrap_or_default();
        let index_manager = IndexManager::from_indexes(
            collection.resolved_root_id(),
            collection.schema(),
            collection.write_indexes(),
        )
        .map_err(|error| {
            query::error::QueryError::execution(format!("failed to create index manager: {error}"))
        })?;
        let result: query::error::Result<(Document, usize, Cid, Bytes, Option<(Cid, Bytes)>)> =
            async {
                self.db
                    .validate_downsample_write(
                        &datastore,
                        &systemstore,
                        &collection,
                        &current_doc,
                        Some(&modified_fields),
                    )
                    .await
                    .map_err(|error| query::error::QueryError::execution(error.to_string()))?;
                let heads = attempt.heads.for_attempt(&headstore, doc_short_id).await?;
                write_local_update(
                    &datastore,
                    &collection,
                    &mut current_doc,
                    doc_short_id,
                    &index_manager,
                    &mut modified_fields,
                    &heads,
                    run,
                    &embedding_config,
                    &inputs,
                )
                .await?;
                let block_result = write_document_blocks_with_inputs(
                    &blockstore,
                    &headstore,
                    &current_doc,
                    collection.schema(),
                    DocRef::new(collection.resolved_root_id(), doc_short_id),
                    Some(&modified_fields),
                    attempt.context.encryption(),
                    attempt.context.signing(),
                    attempt.context.batch_session(),
                    self.db.kms().as_ref(),
                    attempt.prepared_inputs.as_ref(),
                    Some(heads.into_owned()),
                )
                .await
                .map_err(|error| {
                    error.into_query_error_in("failed to write document blocks for update")
                })?;
                register_block_doc_id_mappings(
                    &systemstore,
                    &block_result,
                    &canonical_doc_id.to_string(),
                )
                .await?;
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
                    current_doc.clone(),
                    current_doc.values().len(),
                    block_result.cid,
                    block_result.block,
                    col_data,
                ))
            }
            .await;

        drop(datastore);
        drop(systemstore);
        drop(blockstore);
        drop(headstore);
        let (doc, fields_modified, cid, block, col_data) = match result {
            Ok(result) => result,
            Err(error) => {
                if let Err(discard_error) = txn.discard() {
                    warn!(error = %discard_error, "Failed to discard retry transaction after update error");
                }
                return Err(error);
            }
        };
        if let Some(bus) = self.db.event_bus().cloned() {
            let collection = collection.clone();
            let doc_id = canonical_doc_id.to_string();
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
        let (io, callbacks) = txn
            .into_retry_attempt()
            .map_err(|error| crate::error::commit_query_error(error.into()))?;
        retry
            .finish_attempt_io(io, callbacks)
            .map_err(|error| crate::error::commit_query_error(error.into()))?;
        Ok((doc, fields_modified, Some((cid, block, col_data))))
    }
}

/// What an update attempt returns: the document, the count of fields it
/// modified, and its commit (`None` when it wrote nothing).
type UpdateAttemptOutput = (Document, usize, Option<(Cid, Bytes, Option<(Cid, Bytes)>)>);
