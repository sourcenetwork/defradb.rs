use super::*;
use events::{Message, Update};
use storage::corekv::OnCommit;

use crate::block::builder::ComputedBlocks;
use crate::write::create::{self as pipeline, CreateStores, Created};

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
    docs: Vec<(Document, ComputedBlocks)>,
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

impl<S> storage::corekv::RetryAttemptFn for CreateRetryAttempt<S>
where
    S: Store + 'static,
{
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
        let mut results = self
            .create_many_with_resolved_inputs(context, collection_name, vec![doc])
            .await?;
        results
            .pop()
            .ok_or_else(|| query::error::QueryError::execution("create returned no result"))
    }

    /// Run the create, fetching each wave of missing remote inputs outside
    /// the transaction and running it again, unless the caller already keeps
    /// the memo and resolves the inputs itself.
    pub(super) async fn create_many_with_resolved_inputs(
        &self,
        context: &RequestContext,
        collection_name: &str,
        docs: Vec<Document>,
    ) -> query::error::Result<Vec<CreateResult>> {
        if self.prepared_inputs.is_some() {
            return self.create_impl(context, collection_name, docs).await;
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
                .create_impl(context, collection_name, docs.clone())
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
        let identity = pipeline::identity(&self.db, &collection, doc).await?;
        pipeline::seed_identity(&self.db, doc, identity);
        Ok(())
    }

    /// Form every document, compute all of their blocks, then store them in
    /// one transaction.
    async fn create_impl(
        &self,
        context: &RequestContext,
        collection_name: &str,
        docs: Vec<Document>,
    ) -> query::error::Result<Vec<CreateResult>> {
        if docs.is_empty() {
            return Ok(Vec::new());
        }
        self.db
            .check_node_access_as(context, acp::nac::NodePermission::DocumentUpdate)
            .await
            .map_err(|e| query::error::QueryError::permission_denied(e.to_string()))?;

        let (_collection_guard, collection) = self.guarded_collection(collection_name).await?;

        // Embedding values affect the genesis CID. Every document's missing
        // embeddings join one wave.
        let mut first_miss = None;
        let mut prepared = Vec::with_capacity(docs.len());
        for mut doc in docs {
            match pipeline::embed(
                &self.db,
                &collection,
                &mut doc,
                self.prepared_inputs.as_ref(),
            )
            .await
            {
                Ok(()) => {}
                Err(query::error::QueryError::NeedsInput(request)) => {
                    first_miss.get_or_insert(request);
                }
                Err(error) => return Err(error),
            }
            prepared.push(doc);
        }
        if let Some(request) = first_miss {
            return Err(query::error::QueryError::NeedsInput(request));
        }

        // No per-doc write guards for creates: identities are derived inside
        // the txn; the DocID-mapping duplicate check is the gate.
        let mut identified = Vec::with_capacity(prepared.len());
        for mut doc in prepared {
            let identity = pipeline::identity(&self.db, &collection, &doc).await?;
            pipeline::prepare_keys(&self.db, &mut doc, identity, context).await?;
            identified.push((doc, identity));
        }

        let docs = if identified.len() == 1 {
            let (doc, identity) = identified.pop().expect("one document");
            let blocks = pipeline::compute(
                &collection,
                &doc,
                identity,
                context,
                self.prepared_inputs.as_ref(),
            )?;
            vec![(doc, blocks)]
        } else {
            pipeline::compute_many(
                &self.db,
                &collection,
                identified,
                context,
                self.prepared_inputs.as_ref(),
            )
            .await?
            .into_iter()
            .map(|(doc, _, blocks)| (doc, blocks))
            .collect()
        };

        let acp_registration = self.owed_acp_registration(context, &collection)?;
        let attempt = CreateRetryAttempt {
            db: Arc::clone(&self.db),
            collection_name: collection_name.to_string(),
            collection: collection.clone(),
            docs,
            context: context.clone(),
            acp_registration,
            prepared_inputs: Some(self.prepared_inputs.clone().unwrap_or_default()),
        };
        let result = self
            .db
            .store()
            .transact_retry(self.db.options().retry_options(), Box::new(attempt))
            .await;
        let created = match result {
            Ok(value) => *value
                .downcast::<Vec<Created>>()
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
        Ok(created.into_iter().map(Created::into_result).collect())
    }

    async fn run_create_attempt(
        &self,
        retry: &mut dyn storage::corekv::RetryTxn,
        attempt: &CreateRetryAttempt<S>,
    ) -> query::error::Result<Vec<Created>> {
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

        let result: query::error::Result<Vec<Created>> = async {
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
            let stores = CreateStores {
                datastore: &datastore,
                systemstore: &systemstore,
                blockstore: &blockstore,
                headstore: &headstore,
            };
            let mut created = Vec::with_capacity(attempt.docs.len());
            for (doc, computed) in &attempt.docs {
                created.push(
                    pipeline::persist(
                        &self.db,
                        &stores,
                        &attempt.collection_name,
                        &collection,
                        &index_manager,
                        doc,
                        computed,
                        &attempt.context,
                        attempt.prepared_inputs.as_ref(),
                    )
                    .await?,
                );
            }
            Ok(created)
        }
        .await;

        drop(datastore);
        drop(systemstore);
        drop(blockstore);
        drop(headstore);
        let created = match result {
            Ok(created) => created,
            Err(error) => {
                if let Err(discard_error) = txn.discard() {
                    warn!(error = %discard_error, "Failed to discard retry transaction after create error");
                }
                return Err(error);
            }
        };
        for created in &created {
            if let Some(registration) = attempt.acp_registration.as_ref() {
                txn.on_commit(OnCommit::OrderedAsync(acp_registration_retry_callback(
                    Arc::clone(&registration.acp),
                    registration.creator.clone(),
                    registration.policy_id.clone(),
                    registration.resource_name.clone(),
                    created.doc_id.to_string(),
                    registration.bearer_token.clone(),
                )))
                .map_err(crate::error::commit_query_error)?;
            }
            if let Some(bus) = self.db.event_bus().cloned() {
                let collection_id = collection.collection_id().to_string();
                let doc_id = created.doc_id.to_string();
                let cid = created.cid;
                let block = created.block.clone();
                let collection_block = created.collection_block.clone();
                txn.on_commit(OnCommit::Ordered(Box::new(move || {
                    bus.publish(Message::update(Update::new(
                        doc_id.clone(),
                        cid,
                        collection_id.clone(),
                        block,
                        false,
                        false,
                    )));
                    if let Some((col_cid, col_block)) = collection_block {
                        bus.publish(Message::update(Update::new_with_subject_doc_id(
                            String::new(),
                            doc_id,
                            col_cid,
                            collection_id,
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
        Ok(created)
    }
}
