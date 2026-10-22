//! Mutation execution methods for QueryRunner.

use acp::{DocumentPermission, Identity};
use chrono::{DateTime, FixedOffset, Utc};
use identity::Did;
use rapidhash::{HashMapExt, RapidHashMap};
use serde_json::{Map, Value as JsonValue};
use std::sync::Arc;
use web_time::Instant;

use crate::error::{QueryError, Result};
use crate::mapper::{Mutation, MutationType, Requestable};
use crate::mutator::DocMutator;
use crate::plan::{CreateNode, DeleteNode, UpdateNode, UpsertNode};
use crate::planner::PlanNode;
use crate::query_parse::parse_mutations_with_limits;
use crate::txn::TransactionRegistry;

use super::plan_drive;
use super::{DocFetcher, QueryRunner};

pub(super) struct AuthorizedMutation {
    pub collection: Arc<schema::CollectionVersion>,
    pub mapping: crate::document::DocumentMapping,
    pub resolved_doc_ids: Option<Vec<String>>,
    pub acp_filtered_doc_ids: Option<Vec<String>>,
}

/// The guard that cleared the encryption thread-local when mutations still
/// set it. A mutation's encryption now travels in its `RequestContext`; the
/// guard stays, test-only, for the tests written against it.
#[cfg(test)]
struct EncryptionConfigGuard;

#[cfg(test)]
impl Drop for EncryptionConfigGuard {
    fn drop(&mut self) {
        defra_core::encryption::set_encryption_config(None);
    }
}

fn preserve_acp_input_error(action: &str, doc_id: &str, error: QueryError) -> QueryError {
    match error {
        error @ QueryError::NeedsInput(_) => error,
        error => QueryError::acp_check_failed(action, doc_id, error),
    }
}

impl<F: DocFetcher + 'static, R: TransactionRegistry + 'static> QueryRunner<F, R> {
    pub(crate) async fn check_doc_access_for_read(
        &self,
        identity: &Identity,
        permission: DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool> {
        crate::txn::check_doc_access_with_prepared_inputs(
            self.acp.as_ref(),
            self.acp_registration_projection.as_deref(),
            identity,
            permission,
            policy_id,
            resource_name,
            doc_id,
            self.retry_inputs.as_ref(),
        )
        .await
    }

    /// Execute a GraphQL mutation and return JSON results.
    ///
    /// Requires a mutator to be configured via `with_mutator()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let runner = QueryRunner::new(fetcher, collections)
    ///     .with_mutator(mutator);
    ///
    /// let result = runner.execute_mutation(r#"
    ///     mutation {
    ///         create_Users(input: [{name: "Alice", age: 30}]) {
    ///             _docID
    ///             name
    ///         }
    ///     }
    /// "#).await?;
    /// ```
    pub async fn execute_mutation(&self, mutation_str: &str) -> Result<JsonValue> {
        self.execute_mutation_with_identity(mutation_str, None)
            .await
    }

    /// Execute a GraphQL mutation with identity for ACP permission checks.
    ///
    /// For collections with ACP policies:
    /// - CREATE: Registers created documents with caller_identity as owner (if caller_identity provided)
    /// - UPDATE: Checks caller_identity has updater permission on each document
    /// - DELETE: Checks caller_identity has deleter permission on each document
    pub async fn execute_mutation_with_identity(
        &self,
        mutation_str: &str,
        caller_identity: Option<Did>,
    ) -> Result<JsonValue> {
        self.execute_mutation_with_identity_and_vars(mutation_str, caller_identity, None)
            .await
    }

    /// Execute a GraphQL mutation with identity and variables.
    pub async fn execute_mutation_with_identity_and_vars(
        &self,
        mutation_str: &str,
        caller_identity: Option<Did>,
        variables: Option<&RapidHashMap<String, JsonValue>>,
    ) -> Result<JsonValue> {
        let mutator = self.mutator.as_ref().ok_or_else(|| {
            QueryError::execution("mutations require a mutator; call with_mutator() first")
        })?;

        let mutations = parse_mutations_with_limits(mutation_str, variables, self.query_limits)?;
        // What this request writes is created by its caller.
        let runner = self.for_caller(caller_identity.as_ref());
        let mut request = crate::executor::QueryRequest::new(mutation_str)
            .with_context(runner.context().into_owned());
        request.identity = caller_identity.clone();
        request.variables = variables.map(|values| {
            JsonValue::Object(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            )
        });
        let execution = Box::pin(async {
            let mut prepared = runner.prepare_mutations(&request).await?;
            loop {
                let attempt_mutator = prepared
                    .as_ref()
                    .and_then(|prepared| mutator.with_prepared_inputs(prepared.inputs.clone()))
                    .unwrap_or_else(|| mutator.clone());
                match runner
                    .execute_parsed_mutations(
                        mutations.clone(),
                        attempt_mutator,
                        caller_identity.clone(),
                        None,
                        prepared.as_deref(),
                    )
                    .await
                {
                    Err(QueryError::NeedsInput(input)) => {
                        prepared = crate::executor::QueryExecutor::resolve_retry_input(
                            &runner, &request, prepared, &input,
                        )
                        .await?;
                    }
                    result => return result,
                }
            }
        });
        execution.await
    }

    /// Execute a GraphQL mutation with a specific mutator, caller_identity, and variables.
    pub(crate) async fn execute_mutation_internal_with_vars(
        &self,
        mutation_str: &str,
        mutator: Arc<dyn DocMutator>,
        caller_identity: Option<Did>,
        variables: Option<&RapidHashMap<String, JsonValue>>,
        fetcher_override: Option<Arc<dyn crate::fetcher::DocFetcher>>,
    ) -> Result<JsonValue> {
        let mutations = parse_mutations_with_limits(mutation_str, variables, self.query_limits)?;
        // What this request writes is created by its caller.
        let runner = self.for_caller(caller_identity.as_ref());
        // Keep the large mutation state machine off the Tokio worker stack.
        Box::pin(runner.execute_parsed_mutations(
            mutations,
            mutator,
            caller_identity,
            fetcher_override,
            None,
        ))
        .await
    }

    /// Execute pre-parsed mutations, skipping redundant GraphQL parsing.
    pub(crate) async fn execute_parsed_mutations(
        &self,
        mutations: Vec<Mutation>,
        mutator: Arc<dyn DocMutator>,
        caller_identity: Option<Did>,
        fetcher_override: Option<Arc<dyn crate::fetcher::DocFetcher>>,
        prepared: Option<&crate::prepared::PreparedMutations>,
    ) -> Result<JsonValue> {
        if mutations
            .iter()
            .any(|mutation| mutation.mutation_type == MutationType::Truncate)
        {
            if mutations.len() != 1 || mutations[0].mutation_type != MutationType::Truncate {
                return Err(QueryError::parse(
                    "truncate mutation must be the only field in an operation",
                ));
            }
            if fetcher_override.is_some() {
                return Err(QueryError::execution(
                    "truncate mutation cannot run in a transaction",
                ));
            }

            let mutation = &mutations[0];
            let truncator = self.collection_truncator.as_ref().ok_or_else(|| {
                QueryError::execution(
                    "truncate mutations require a collection truncator; call with_collection_truncator() first",
                )
            })?;
            truncator
                .truncate_as(
                    &self.context(),
                    &mutation.collection_name,
                    mutation.filter.clone(),
                )
                .await?;

            return Ok(serde_json::json!({ mutation.output_name(): true }));
        }

        // Compute request time once for all mutations in this request.
        // This ensures UTC_NOW resolves to the same timestamp across all mutations,
        // matching Go DefraDB's behavior.
        let utc_offset = FixedOffset::east_opt(0).unwrap();
        let request_time =
            prepared.map_or_else(|| Utc::now().with_timezone(&utc_offset), |p| p.request_time);
        let mut created_ids = RapidHashMap::default();

        // Run eligible implicit mutations as one engine-owned retry unit so
        // selection and expected-value reads are rebuilt after a conflict.
        // Explicit `/tx` requests have their own transaction and are never
        // replayed here. The prepared capsule records whether all policy and
        // retry-boundary checks allowed engine ownership.
        if fetcher_override.is_none()
            && prepared.is_some_and(|prepared| prepared.engine_managed_retry)
        {
            if let Some(batch) = mutator.begin_batch().await? {
                let runner = Arc::new(self.scoped_runner(self.acp.clone()));
                let mutations = Arc::new(mutations);
                let caller_identity = caller_identity.clone();
                let prepared = prepared.cloned();
                let acp = self.acp.clone();
                let operation: crate::mutator::MutationBatchOperation =
                    Arc::new(move |batch_mutator, batch_fetcher| {
                        let runner = runner.clone();
                        let mutations = mutations.clone();
                        let caller_identity = caller_identity.clone();
                        let prepared = prepared.clone();
                        let acp = acp.clone();
                        Box::pin(async move {
                            batch_mutator.set_document_acp(acp.clone());
                            let attempt_acp = batch_fetcher.scoped_acp(acp.clone()).await?;
                            let mut batch_runner = runner.scoped_runner(attempt_acp);
                            batch_runner.acp_registration_projection =
                                batch_mutator.acp_registration_projection();
                            batch_runner.retry_inputs = prepared.as_ref().map(|p| p.inputs.clone());
                            let mut created_ids = RapidHashMap::default();
                            let mut results = Map::new();

                            for mutation in mutations.iter() {
                                let result = batch_runner
                                    .execute_single_mutation(
                                        mutation,
                                        batch_mutator.clone(),
                                        caller_identity.clone(),
                                        request_time,
                                        Some(batch_fetcher.clone()),
                                        prepared
                                            .as_ref()
                                            .and_then(|p| p.mutations.get(&mutation.output_name())),
                                        prepared.as_ref().map(|p| p.inputs.clone()),
                                        &mut created_ids,
                                        true,
                                    )
                                    .await?;
                                results.insert(mutation.output_name(), result);
                            }
                            Ok(JsonValue::Object(results))
                        })
                    });
                return batch.run(operation).await;
            }
        }

        let mut results = Map::new();

        for mutation in mutations {
            let result = self
                .execute_single_mutation(
                    &mutation,
                    mutator.clone(),
                    caller_identity.clone(),
                    request_time,
                    fetcher_override.clone(),
                    prepared.and_then(|p| p.mutations.get(&mutation.output_name())),
                    prepared.map(|p| p.inputs.clone()),
                    &mut created_ids,
                    false,
                )
                .await?;
            // Use alias if provided, otherwise full mutation name (e.g., "add_Users")
            let key = mutation.output_name();
            results.insert(key, result);
        }

        Ok(JsonValue::Object(results))
    }

    #[allow(clippy::too_many_arguments)]
    async fn check_doc_access_prepared(
        &self,
        identity: &Identity,
        permission: DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
        prepared_inputs: Option<&crate::prepared::PreparedInputMemo>,
        strict_attempt: bool,
    ) -> Result<bool> {
        let result = crate::txn::check_doc_access_with_prepared_inputs(
            self.acp.as_ref(),
            self.acp_registration_projection.as_deref(),
            identity,
            permission,
            policy_id,
            resource_name,
            doc_id,
            prepared_inputs,
        )
        .await;
        match result {
            Err(QueryError::NeedsInput(request)) if !strict_attempt => {
                let allowed = self
                    .acp
                    .check_doc_access(identity, permission, policy_id, resource_name, doc_id)
                    .await
                    .map_err(|error| QueryError::execution(error.to_string()))?;
                let Some(inputs) = prepared_inputs else {
                    return Ok(allowed);
                };
                let response = serde_json::to_vec(&allowed)
                    .map_err(|error| QueryError::execution(error.to_string()))?;
                // Every use in the request shares the first answer.
                let answer = inputs.insert(request, std::sync::Arc::<[u8]>::from(response));
                serde_json::from_slice(&answer)
                    .map_err(|error| QueryError::execution(error.to_string()))
            }
            result => result,
        }
    }

    pub(super) async fn authorize_mutation(
        &self,
        mutation: &Mutation,
        caller_identity: &Option<Did>,
        fetcher: &dyn crate::fetcher::DocFetcher,
        projected_ids: Option<&rapidhash::RapidHashSet<String>>,
        prepared_inputs: Option<&crate::prepared::PreparedInputMemo>,
        strict_attempt: bool,
    ) -> Result<AuthorizedMutation> {
        // Validate collection exists - resolve on-demand from provider
        let collection = self.get_collection(&mutation.collection_name).await?;

        // Validate encryptFields if present
        // Match Go's validation order: check existence first, then builtin prefix.
        // _docID is in the field list (passes existence, hits builtin check).
        // _version is NOT in the field list (fails existence check).
        for field_name in &mutation.encrypt_fields {
            let exists = collection.fields.iter().any(|f| f.name == *field_name);
            if !exists {
                return Err(QueryError::execution(format!(
                    "the given field does not exist. Name: {}",
                    field_name
                )));
            }
            if field_name.starts_with('_') {
                return Err(QueryError::execution(format!(
                    "can not encrypt build-in field. Name: {}",
                    field_name
                )));
            }
        }

        // Build document mapping from requested fields
        let mapping = self.build_mutation_mapping(mutation)?;

        // Resolve filter to doc_ids if filter is provided without doc_ids
        let resolved_doc_ids = self.resolve_filter_to_doc_ids(mutation, fetcher).await?;

        // Get doc_ids for permission checking (UPDATE/DELETE need this)
        let doc_ids_for_check = resolved_doc_ids
            .as_ref()
            .or(mutation.doc_ids.as_ref())
            .cloned();

        // Two-phase ACP permission check for UPDATE/DELETE operations.
        //
        // Phase 1: Check READ permission. If denied, the document is invisible to
        // this identity -- silently remove it from the target set (empty results, no error).
        // Phase 2: Check UPDATE/DELETE permission. If denied but readable, the document
        // is visible but unauthorized -- return an error.
        //
        // This matches Go's behavior where GQL mutations on invisible documents return
        // empty results, while mutations on visible-but-unauthorized documents return errors.
        if let Some(validator) = &self.write_validator {
            let validation_ids: Vec<_> = doc_ids_for_check
                .iter()
                .flatten()
                .filter(|id| !projected_ids.is_some_and(|ids| ids.contains(*id)))
                .cloned()
                .collect();
            let (update_input, update_ops) = if mutation.mutation_type == MutationType::Update {
                mutation.split_update(&collection)?
            } else {
                (mutation.update_input.clone(), RapidHashMap::new())
            };
            let request = crate::access_hooks::WriteRequest {
                identity: caller_identity.as_ref(),
                collection: &collection,
                kind: mutation.mutation_type,
                doc_ids: &validation_ids,
                create_input: &mutation.create_input,
                update_input: &update_input,
                update_ops: &update_ops,
            };
            if strict_attempt {
                let exact_request = serde_json::to_vec(&(
                    Arc::as_ptr(validator) as *const () as usize,
                    caller_identity.as_ref().map(ToString::to_string),
                    &collection.name,
                    &collection.version_id,
                    mutation.mutation_type.as_prefix(),
                    &validation_ids,
                    &mutation.create_input,
                    &update_input,
                    &update_ops,
                ))
                .map_err(|error| QueryError::execution(error.to_string()))?;
                let prepared_request = crate::prepared::PreparedInputRequest::new(
                    crate::prepared::PreparedInputKind::WriteValidation,
                    exact_request,
                );
                let inputs = prepared_inputs.ok_or_else(|| {
                    QueryError::execution("write validation attempt has no request memo")
                })?;
                if inputs.get(&prepared_request).is_none() {
                    return Err(QueryError::NeedsInput(prepared_request));
                }
            } else {
                validator
                    .validate_write(&request)
                    .await
                    .map_err(QueryError::execution)?;
            }
        }

        let mut acp_filtered_doc_ids: Option<Vec<String>> = None;
        if let Some(ref policy) = collection.policy {
            match mutation.mutation_type {
                MutationType::Update | MutationType::Upsert => {
                    if let Some(ref doc_ids) = doc_ids_for_check {
                        let identity_for_acp = Identity::from(caller_identity.as_ref());
                        let mut visible_doc_ids = Vec::new();
                        for doc_id in doc_ids {
                            // Projected creates have no ACP object yet. Execution checks
                            // the real ID after its preceding create has registered it.
                            if projected_ids.is_some_and(|ids| ids.contains(doc_id)) {
                                visible_doc_ids.push(doc_id.clone());
                                continue;
                            }
                            // Phase 1: Check if the identity can read the document
                            let can_read = self
                                .check_doc_access_prepared(
                                    &identity_for_acp,
                                    DocumentPermission::Read,
                                    &policy.id,
                                    &policy.resource_name,
                                    doc_id,
                                    prepared_inputs,
                                    strict_attempt,
                                )
                                .await
                                .map_err(|error| preserve_acp_input_error("read", doc_id, error))?;

                            if !can_read {
                                // Document is invisible to this identity -- skip silently
                                continue;
                            }

                            // Phase 2: Check if the identity can update the document
                            let can_update = self
                                .check_doc_access_prepared(
                                    &identity_for_acp,
                                    DocumentPermission::Update,
                                    &policy.id,
                                    &policy.resource_name,
                                    doc_id,
                                    prepared_inputs,
                                    strict_attempt,
                                )
                                .await
                                .map_err(|error| match error {
                                    error @ QueryError::NeedsInput(_) => error,
                                    error => QueryError::acp_check_failed("update", doc_id, error),
                                })?;

                            if !can_update {
                                return Err(QueryError::document_not_found(
                                    "document not found or not authorized to access",
                                ));
                            }
                            visible_doc_ids.push(doc_id.clone());
                        }
                        acp_filtered_doc_ids = Some(visible_doc_ids);
                    }
                }
                MutationType::Delete => {
                    if let Some(ref doc_ids) = doc_ids_for_check {
                        let identity_for_acp = Identity::from(caller_identity.as_ref());
                        let mut visible_doc_ids = Vec::new();
                        for doc_id in doc_ids {
                            // Projected creates have no ACP object yet. Execution checks
                            // the real ID after its preceding create has registered it.
                            if projected_ids.is_some_and(|ids| ids.contains(doc_id)) {
                                visible_doc_ids.push(doc_id.clone());
                                continue;
                            }
                            // Phase 1: Check if the identity can read the document
                            let can_read = self
                                .check_doc_access_prepared(
                                    &identity_for_acp,
                                    DocumentPermission::Read,
                                    &policy.id,
                                    &policy.resource_name,
                                    doc_id,
                                    prepared_inputs,
                                    strict_attempt,
                                )
                                .await
                                .map_err(|error| preserve_acp_input_error("read", doc_id, error))?;

                            if !can_read {
                                // Document is invisible to this identity -- skip silently
                                continue;
                            }

                            // Phase 2: Check if the identity can delete the document
                            let can_delete = self
                                .check_doc_access_prepared(
                                    &identity_for_acp,
                                    DocumentPermission::Delete,
                                    &policy.id,
                                    &policy.resource_name,
                                    doc_id,
                                    prepared_inputs,
                                    strict_attempt,
                                )
                                .await
                                .map_err(|error| match error {
                                    error @ QueryError::NeedsInput(_) => error,
                                    error => QueryError::acp_check_failed("delete", doc_id, error),
                                })?;

                            if !can_delete {
                                return Err(QueryError::document_not_found(
                                    "document not found or not authorized to access",
                                ));
                            }
                            visible_doc_ids.push(doc_id.clone());
                        }
                        acp_filtered_doc_ids = Some(visible_doc_ids);
                    }
                }
                MutationType::Create => {
                    // CREATE permission is checked implicitly -- anyone can create
                    // but ownership is established via registration after the write.
                }
                MutationType::Truncate => unreachable!("truncate bypasses document ACP checks"),
            }
        }

        Ok(AuthorizedMutation {
            collection,
            mapping,
            resolved_doc_ids,
            acp_filtered_doc_ids,
        })
    }

    /// Execute a single mutation operation with ACP enforcement.
    #[allow(clippy::too_many_arguments)]
    async fn execute_single_mutation(
        &self,
        mutation: &Mutation,
        mutator: Arc<dyn DocMutator>,
        caller_identity: Option<Did>,
        request_time: DateTime<FixedOffset>,
        fetcher_override: Option<Arc<dyn crate::fetcher::DocFetcher>>,
        prepared: Option<&crate::prepared::PreparedMutation>,
        prepared_inputs: Option<crate::prepared::PreparedInputMemo>,
        created_ids: &mut RapidHashMap<String, String>,
        strict_attempt: bool,
    ) -> Result<JsonValue> {
        let fetcher: Arc<dyn crate::fetcher::DocFetcher> =
            fetcher_override.unwrap_or_else(|| self.fetcher.clone());
        let mut selected_mutation = mutation.clone();
        let mut authorization = None;
        if let Some(ids) = prepared.and_then(|prepared| prepared.doc_ids.as_ref()) {
            let mut ids: Vec<_> = ids
                .iter()
                .map(|id| created_ids.get(id).unwrap_or(id).clone())
                .collect();
            if mutation.doc_ids.is_none()
                && mutation.filter.is_some()
                && matches!(
                    mutation.mutation_type,
                    MutationType::Delete | MutationType::Upsert
                )
            {
                let mut current = self
                    .authorize_mutation(
                        mutation,
                        &caller_identity,
                        fetcher.as_ref(),
                        None,
                        prepared_inputs.as_ref(),
                        strict_attempt,
                    )
                    .await?;
                let matching = current
                    .acp_filtered_doc_ids
                    .as_ref()
                    .or(current.resolved_doc_ids.as_ref())
                    .cloned()
                    .unwrap_or_default();
                if mutation.mutation_type == MutationType::Upsert
                    && (matching.len() != ids.len() || matching.iter().any(|id| !ids.contains(id)))
                {
                    return Err(QueryError::execution(
                        "upsert target changed after key preparation",
                    ));
                }
                ids.retain(|id| matching.contains(id));
                current.resolved_doc_ids = Some(ids.clone());
                if current.acp_filtered_doc_ids.is_some() {
                    current.acp_filtered_doc_ids = Some(ids.clone());
                }
                authorization = Some(current);
            }
            selected_mutation.doc_ids = Some(ids);
        }
        let mutation = &selected_mutation;
        let AuthorizedMutation {
            collection,
            mapping,
            resolved_doc_ids,
            acp_filtered_doc_ids,
        } = match authorization {
            Some(authorization) => authorization,
            None => {
                self.authorize_mutation(
                    mutation,
                    &caller_identity,
                    fetcher.as_ref(),
                    None,
                    prepared_inputs.as_ref(),
                    strict_attempt,
                )
                .await?
            }
        };

        // This mutation's own encryption and, for a remote signer, the access
        // decision its creates are signed under, so neither reaches another one.
        let mut context = self
            .context()
            .into_owned()
            .with_encryption(mutation.encryption());

        if matches!(mutation.mutation_type, MutationType::Create) {
            match context.signing().cloned() {
                Some(mut config) if config.remote_signer.is_some() => {
                    tracing::info!(
                        collection = %mutation.collection_name,
                        caller_identity = ?caller_identity.as_ref().map(|d| d.to_string()),
                        policy_id = ?collection.policy.as_ref().map(|p| p.id.clone()),
                        resource = ?collection.policy.as_ref().map(|p| p.resource_name.clone()),
                        "create mutation entering remote signer authorization path"
                    );
                    if let (Some(identity_did), Some(policy)) =
                        (caller_identity.as_ref(), collection.policy.as_ref())
                    {
                        tracing::info!(
                            actor_did = %identity_did,
                            policy_id = %policy.id,
                            resource = %policy.resource_name,
                            object_id = %policy.resource_name,
                            permission = "writer",
                            "create mutation requesting access decision"
                        );
                        let request = crate::prepared::PreparedInputRequest::new(
                            crate::prepared::PreparedInputKind::AcpDecision,
                            serde_json::to_vec(&(
                                identity_did.as_str(),
                                &policy.id,
                                &policy.resource_name,
                                &policy.resource_name,
                                "writer",
                                self.acp.bearer_token_for(identity_did.as_str()),
                            ))
                            .map_err(|error| QueryError::execution(error.to_string()))?,
                        );
                        let decision_id = if let Some(bytes) = prepared_inputs
                            .as_ref()
                            .and_then(|inputs| inputs.get(&request))
                        {
                            serde_json::from_slice::<Option<String>>(&bytes).map_err(|error| {
                                QueryError::execution(format!(
                                    "cached create authorization response is invalid: {error}"
                                ))
                            })?
                        } else {
                            if strict_attempt {
                                return Err(QueryError::NeedsInput(request));
                            }
                            let decision_id = self
                                .acp
                                .create_access_decision(
                                    &Identity::from(identity_did.clone()),
                                    &policy.id,
                                    &policy.resource_name,
                                    &policy.resource_name,
                                    "writer",
                                )
                                .await
                                .map_err(|e| {
                                    QueryError::permission_denied(format!(
                                        "create authorization failed: {}",
                                        e
                                    ))
                                })?;
                            match prepared_inputs.as_ref() {
                                Some(inputs) => {
                                    let response =
                                        serde_json::to_vec(&decision_id).map_err(|error| {
                                            QueryError::execution(error.to_string())
                                        })?;
                                    // Every use in the request shares the first answer.
                                    let answer = inputs
                                        .insert(request, std::sync::Arc::<[u8]>::from(response));
                                    serde_json::from_slice(&answer)
                                        .map_err(|error| QueryError::execution(error.to_string()))?
                                }
                                None => decision_id,
                            }
                        };

                        tracing::info!(
                            actor_did = %identity_did,
                            policy_id = %policy.id,
                            resource = %policy.resource_name,
                            object_id = %policy.resource_name,
                            permission = "writer",
                            decision_id = ?decision_id,
                            "create mutation access decision result"
                        );
                        if let Some(decision_id) = decision_id {
                            config.signing_authorization =
                                Some(defra_core::signing::SigningAuthorization::Decision {
                                    decision_id,
                                });
                            context = context.with_signing(Some(config));
                        } else {
                            tracing::warn!(
                                collection = %mutation.collection_name,
                                "create mutation received no access decision for remote signer"
                            );
                        }
                    } else {
                        tracing::warn!(
                            collection = %mutation.collection_name,
                            has_acp = true,
                            has_caller_identity = caller_identity.is_some(),
                            has_policy = collection.policy.is_some(),
                            "create mutation skipped remote signer authorization due to missing prerequisites"
                        );
                    }
                }
                Some(_) => {
                    tracing::info!(
                        collection = %mutation.collection_name,
                        "create mutation using local signer; remote signer authorization skipped"
                    );
                }
                None => {
                    // No signing config is the normal case for nodes that don't
                    // run with `--signer-type=...`. There is nothing to authorize
                    // and nothing actionable for an operator to do, so this stays
                    // at debug. The two `warn!` arms above remain warnings because
                    // they signal a *partially* configured remote-signer setup.
                    tracing::debug!(
                        collection = %mutation.collection_name,
                        "create mutation has no signing config; remote signer authorization skipped"
                    );
                }
            }
        }

        let bound_prepared = prepared.map(|prepared| {
            let mut prepared = prepared.clone();
            prepared.bind_created(created_ids);
            prepared
        });
        let prepared = bound_prepared.as_ref();

        // Build and execute the appropriate mutation plan
        let mut plan: Box<dyn PlanNode> = match mutation.mutation_type {
            MutationType::Create => {
                let inputs = self.build_create_inputs(mutation, &collection)?;
                Box::new(
                    CreateNode::new(&mutation.collection_name, mutator.clone(), mapping.clone())
                        .with_context(context.clone())
                        .with_collection(collection.clone())
                        .with_request_time(request_time)
                        .with_inputs(inputs)
                        .with_prepared_documents(prepared.map(|p| p.creates.clone())),
                )
            }
            MutationType::Update => {
                let input = self.build_update_input(mutation, &collection)?;
                let mut node = UpdateNode::new(
                    &mutation.collection_name,
                    mutator.clone(),
                    fetcher.clone(),
                    mapping.clone(),
                )
                .with_context(context.clone())
                .with_collection(collection.clone())
                .with_request_time(request_time)
                .with_input(input)
                .with_prepared_writes(prepared.map(|p| p.updates.clone()))
                .with_prepared_update_targets(prepared.map(|p| p.update_targets.clone()))
                .with_prepared_embeddings(
                    prepared.map(|p| p.embeddings.clone()),
                    prepared_inputs.clone(),
                );

                // Use ACP-filtered doc_ids (invisible docs removed), or resolved/original
                if let Some(ref doc_ids) = acp_filtered_doc_ids {
                    node = node.with_doc_ids(doc_ids.clone());
                } else if let Some(ref doc_ids) = resolved_doc_ids {
                    node = node.with_doc_ids(doc_ids.clone());
                } else if let Some(ref doc_ids) = mutation.doc_ids {
                    node = node.with_doc_ids(doc_ids.clone());
                }

                // Keep the filter even when it was already resolved to IDs.
                // UpdateNode revalidates it against the document snapshot that
                // is passed to the mutator, closing the selection/write race
                // without filtering the post-update result.
                if let Some(ref filter) = mutation.filter {
                    node = node.with_filter(filter.clone());
                }

                Box::new(node)
            }
            MutationType::Delete => {
                let mut node = DeleteNode::new(
                    &mutation.collection_name,
                    mutator.clone(),
                    fetcher.clone(),
                    mapping.clone(),
                )
                .with_context(context.clone());

                // Use ACP-filtered doc_ids (invisible docs removed), or resolved/original
                if let Some(ref doc_ids) = acp_filtered_doc_ids {
                    node = node.with_doc_ids(doc_ids.clone());
                } else if let Some(ref doc_ids) = resolved_doc_ids {
                    node = node.with_doc_ids(doc_ids.clone());
                } else if let Some(ref doc_ids) = mutation.doc_ids {
                    node = node.with_doc_ids(doc_ids.clone());
                }

                // Pass through filter for node-level resolution when no doc_ids resolved
                if mutation.filter.is_some()
                    && resolved_doc_ids.is_none()
                    && mutation.doc_ids.is_none()
                {
                    node = node.with_filter(mutation.filter.clone().unwrap());
                }

                Box::new(node)
            }
            MutationType::Upsert => {
                let mut node =
                    UpsertNode::new(&mutation.collection_name, mutator.clone(), mapping.clone())
                        .with_context(context.clone())
                        .with_collection(collection.clone())
                        .with_request_time(request_time)
                        .with_prepared_writes(prepared.cloned());

                // Set create_input (from Go's 'add' argument)
                if !mutation.create_input.is_empty() {
                    let create_input =
                        self.build_upsert_input_from_map(&collection, &mutation.create_input[0])?;
                    node = node.with_create_input(create_input);
                }

                // Set update_input (from Go's 'update' argument)
                if !mutation.update_input.is_empty() {
                    let update_input =
                        self.build_upsert_input_from_map(&collection, &mutation.update_input)?;
                    node = node.with_update_input(update_input);
                }

                // Use ACP-filtered doc_ids (invisible docs removed), or resolved/original
                if let Some(ref doc_ids) = acp_filtered_doc_ids {
                    node = node.with_doc_ids(doc_ids.clone());
                } else if let Some(ref doc_ids) = resolved_doc_ids {
                    node = node.with_doc_ids(doc_ids.clone());
                } else if let Some(ref doc_ids) = mutation.doc_ids {
                    node = node.with_doc_ids(doc_ids.clone());
                }

                Box::new(node)
            }
            MutationType::Truncate => unreachable!("truncate bypasses document mutation plans"),
        };

        // Execute the plan.
        // When ACP is active, wrap DocumentNotFound errors to match Go's generic message
        // (security best practice: don't reveal whether document exists vs unauthorized).
        let has_acp = collection.policy.is_some();
        let map_doc_not_found = |e: QueryError| -> QueryError {
            if has_acp {
                match &e {
                    QueryError::DocumentNotFound(_) => {
                        return QueryError::document_not_found(
                            "document not found or not authorized to access",
                        );
                    }
                    QueryError::Execution(msg) if msg.contains("document not found:") => {
                        return QueryError::document_not_found(
                            "document not found or not authorized to access",
                        );
                    }
                    _ => {}
                }
            }
            e
        };

        let plan_execution_start = Instant::now();
        let outcome = async {
            plan.init().await.map_err(map_doc_not_found)?;
            plan.start().await.map_err(map_doc_not_found)?;

            let mut results = Vec::new();
            let mut result_doc_ids = Vec::new();

            while plan.next().await.map_err(map_doc_not_found)? {
                let doc = plan.value();
                if let Some(doc_id) = doc.doc_id() {
                    result_doc_ids.push(doc_id.to_string());
                }
                let json = self.doc_to_json(doc, &mapping)?;
                results.push(json);
            }

            Ok((results, result_doc_ids))
        }
        .await;

        let (mut results, result_doc_ids) = plan_drive::close_after(plan.as_mut(), outcome)
            .await
            .map_err(map_doc_not_found)?;

        if let Some(prepared) = prepared {
            for (created, actual) in prepared.creates.iter().zip(&result_doc_ids) {
                if let Some(write) = created.write_preparation() {
                    created_ids.insert(
                        crate::prepared::prepared_doc_id(write).to_string(),
                        actual.clone(),
                    );
                }
            }
        }

        let plan_execution_elapsed = plan_execution_start.elapsed();
        for doc_id in &result_doc_ids {
            tracing::info!(
                doc_id = %doc_id,
                elapsed = ?plan_execution_elapsed,
                "Mutation plan execution completed"
            );
        }

        // External ACP cannot join the transaction: a create owes its
        // registration through a record written in its own transaction (see
        // `acp::pending`), and the ordered hook settles that record once the
        // commit is durable. A document an upsert found and updated owes
        // nothing, so its caller never becomes its owner.
        if !self.acp.supports_transactions()
            && !mutator.manages_external_acp_hooks()
            && matches!(
                mutation.mutation_type,
                MutationType::Create | MutationType::Upsert
            )
        {
            if let (Some(policy), Some(identity_did), Some(projection)) = (
                collection.policy.as_ref(),
                caller_identity.as_ref(),
                self.acp_registration_projection.as_ref(),
            ) {
                for result in &results {
                    if let Some(doc_id) = result.get("_docID").and_then(|value| value.as_str()) {
                        let owed = projection
                            .owed_creator(&policy.id, &policy.resource_name, doc_id)
                            .await
                            .map_err(|error| {
                                QueryError::acp_registration_check_failed(doc_id, error)
                            })?;
                        if owed.is_none() {
                            continue;
                        }

                        let acp = self.acp.clone();
                        let identity = identity_did.clone();
                        let policy_id = policy.id.clone();
                        let resource_name = policy.resource_name.clone();
                        let doc_id = doc_id.to_string();
                        let token = self.acp.bearer_token_for(identity_did.as_str());
                        let callback: storage::corekv::CommitHook = Box::new(move || {
                            let acp = acp.clone();
                            let identity = identity.clone();
                            let policy_id = policy_id.clone();
                            let resource_name = resource_name.clone();
                            let doc_id = doc_id.clone();
                            let token = token.clone();
                            Box::pin(async move {
                                acp::with_request_bearer(
                                    &acp,
                                    Some(identity.to_string()),
                                    token,
                                    |acp| async move {
                                        acp.redrive_registration(
                                            &policy_id,
                                            &resource_name,
                                            &doc_id,
                                        )
                                        .await
                                        .map_err(|error| {
                                            format!(
                                                "failed to register ACP document {doc_id}: {error}"
                                            )
                                        })
                                    },
                                )
                                .await
                            })
                        });
                        mutator.register_ordered_async_hook(callback).await?;
                    }
                }
            } else if let (Some(policy), Some(identity_did), MutationType::Create) = (
                collection.policy.as_ref(),
                caller_identity.as_ref(),
                &mutation.mutation_type,
            ) {
                // A mutator with no transaction behind it records nothing owed;
                // only a create's results are documents this request made.
                for result in &results {
                    if let Some(doc_id) = result.get("_docID").and_then(|value| value.as_str()) {
                        if !self
                            .acp
                            .is_doc_registered(&policy.id, &policy.resource_name, doc_id)
                            .await
                            .map_err(|error| {
                                QueryError::acp_registration_check_failed(doc_id, error)
                            })?
                        {
                            self.acp
                                .register_doc_object(
                                    identity_did,
                                    &policy.id,
                                    &policy.resource_name,
                                    doc_id,
                                )
                                .await
                                .map_err(|error| {
                                    QueryError::acp_registration_failed(doc_id, error)
                                })?;
                        }
                    }
                }
            }
        }

        // A local ACP provider already wrote registration in the authorized
        // transaction. External unregistration is a fallible ordered hook.
        if !mutator.manages_external_acp_hooks()
            && matches!(mutation.mutation_type, MutationType::Delete)
        {
            if let Some(policy) = collection.policy.as_ref() {
                for result in &results {
                    if let Some(doc_id) = result.get("_docID").and_then(|value| value.as_str()) {
                        if self.acp.supports_transactions() {
                            self.acp
                                .unregister_doc_object(&policy.id, &policy.resource_name, doc_id)
                                .await
                                .map_err(|error| QueryError::execution(error.to_string()))?;
                        } else if self.acp_registration_projection.is_some() {
                            let acp = self.acp.clone();
                            let policy_id = policy.id.clone();
                            let resource_name = policy.resource_name.clone();
                            let doc_id = doc_id.to_string();
                            let identity = caller_identity.clone();
                            let token = identity
                                .as_ref()
                                .and_then(|identity| self.acp.bearer_token_for(identity.as_str()));
                            let callback: storage::corekv::CommitHook = Box::new(move || {
                                let acp = acp.clone();
                                let policy_id = policy_id.clone();
                                let resource_name = resource_name.clone();
                                let doc_id = doc_id.clone();
                                let identity = identity.clone();
                                let token = token.clone();
                                Box::pin(async move {
                                    acp::with_request_bearer(
                                        &acp,
                                        identity.map(|identity| identity.to_string()),
                                        token,
                                        |acp| async move {
                                            acp.unregister_doc_object(
                                                &policy_id,
                                                &resource_name,
                                                &doc_id,
                                            )
                                            .await
                                            .map_err(|error| {
                                                format!("failed to unregister ACP document {doc_id}: {error}")
                                            })
                                        },
                                    )
                                    .await
                                })
                            });
                            mutator.register_ordered_async_hook(callback).await?;
                        } else if let Err(error) = self
                            .acp
                            .unregister_doc_object(&policy.id, &policy.resource_name, doc_id)
                            .await
                        {
                            tracing::warn!(%doc_id, policy_id = %policy.id, %error, "Failed to unregister deleted document from ACP - orphaned registration may remain");
                        }
                    }
                }
            }
        }

        // Enrich results with _version data if requested.
        // _version is a nested select (Requestable::Select) that returns commit history.
        // This must happen after ACP blocks which also need _docID.
        let version_select = mutation.fields.iter().find_map(|r| {
            if let Requestable::Select(s) = r {
                if s.field.name == "_version" {
                    return Some(s.as_ref());
                }
            }
            None
        });

        if let Some(version_sel) = version_select {
            let fetcher: &dyn crate::fetcher::DocFetcher = fetcher.as_ref();
            let output_name = version_sel.field.output_name().to_string();
            let docid_explicitly_requested = mutation
                .requested_fields()
                .iter()
                .any(|f| f.name == "_docID");

            for result in &mut results {
                if let JsonValue::Object(ref mut obj) = result {
                    if let Some(doc_id) =
                        obj.get("_docID").and_then(|v| v.as_str()).map(String::from)
                    {
                        let version_data = self
                            .fetch_version_data(
                                fetcher,
                                &doc_id,
                                version_sel,
                                &collection.collection_id,
                                None,
                            )
                            .await?;
                        obj.insert(output_name.clone(), version_data);
                    }

                    // Remove _docID from output if it was only added internally for version lookup
                    if !docid_explicitly_requested {
                        obj.remove("_docID");
                    }
                }
            }
        }

        // Enrich results with relation sub-select data if requested.
        //
        // Mutation plan nodes only return flat document fields. When the
        // mutation selection set includes relation sub-selects (e.g.,
        // `update_Author { name published { name } }`), we re-query each
        // result document through the query engine to resolve the joins.
        let relation_selects: Vec<&crate::mapper::Select> = mutation
            .fields
            .iter()
            .filter_map(|r| {
                if let Requestable::Select(s) = r {
                    if s.field.name != "_version" {
                        return Some(s.as_ref());
                    }
                }
                None
            })
            .collect();

        if !relation_selects.is_empty() {
            let docid_explicitly_requested = mutation
                .requested_fields()
                .iter()
                .any(|f| f.name == "_docID");

            for result in &mut results {
                if let JsonValue::Object(ref mut obj) = result {
                    if let Some(doc_id) =
                        obj.get("_docID").and_then(|v| v.as_str()).map(String::from)
                    {
                        // Build a GQL query to fetch relation data for this document
                        let mut relation_fields = String::new();
                        for sel in &relation_selects {
                            relation_fields.push(' ');
                            render_select_to_gql(sel, &mut relation_fields);
                        }

                        let query = format!(
                            "query {{ {}(docID: \"{}\") {{{} }} }}",
                            mutation.collection_name, doc_id, relation_fields
                        );

                        if let Ok(query_result) = self
                            .execute_query_internal(
                                &query,
                                fetcher.as_ref(),
                                caller_identity.clone(),
                            )
                            .await
                        {
                            if let Some(docs) = query_result
                                .get(&mutation.collection_name)
                                .and_then(|v| v.as_array())
                            {
                                if let Some(JsonValue::Object(fetched)) = docs.first() {
                                    for (key, value) in fetched {
                                        obj.insert(key.clone(), value.clone());
                                    }
                                }
                            }
                        }
                    }

                    if !docid_explicitly_requested {
                        obj.remove("_docID");
                    }
                }
            }
        }

        Ok(JsonValue::Array(results))
    }
}

/// Render a Select sub-select back to GQL selection syntax.
fn render_select_to_gql(select: &crate::mapper::Select, out: &mut String) {
    out.push_str(select.field.output_name());
    if !select.fields.is_empty() {
        out.push_str(" { ");
        for child in &select.fields {
            match child {
                Requestable::Field(f) => {
                    out.push_str(f.output_name());
                    out.push(' ');
                }
                Requestable::Select(s) => {
                    render_select_to_gql(s, out);
                    out.push(' ');
                }
                _ => {}
            }
        }
        out.push('}');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use document::{DocID, Document};
    use kovan_queue::seg_queue::SegQueue;
    use rapidhash::RapidHashSet;
    use schema::{CollectionVersion, FieldDescription, FieldKind};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::mutator::{CollectionTruncator, CreateResult, DeleteResult, UpdateResult};
    use crate::test_utils::MockFetcher;
    use crate::{QueryExecutor, QueryRequest};

    struct CapturingMutator {
        created_docs: SegQueue<Document>,
        broadcast_creators: SegQueue<Option<String>>,
        prepared_writes: AtomicUsize,
    }

    #[derive(Default)]
    struct CapturingTruncator {
        calls: SegQueue<(String, bool)>,
    }

    fn drain<T: 'static>(queue: &SegQueue<T>) -> Vec<T> {
        std::iter::from_fn(|| queue.pop()).collect()
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl CollectionTruncator for CapturingTruncator {
        async fn truncate(
            &self,
            collection_name: &str,
            filter: Option<crate::Filter>,
            _identity: Option<&Did>,
        ) -> Result<()> {
            self.calls
                .push((collection_name.to_string(), filter.is_some()));
            Ok(())
        }
    }

    impl CapturingMutator {
        fn new() -> Self {
            Self {
                created_docs: SegQueue::new(),
                broadcast_creators: SegQueue::new(),
                prepared_writes: AtomicUsize::new(0),
            }
        }

        fn created_docs(&self) -> Vec<Document> {
            drain(&self.created_docs)
        }

        fn broadcast_creators(&self) -> Vec<Option<String>> {
            drain(&self.broadcast_creators)
        }
    }

    #[test]
    fn truncate_mutation_forwards_filter_and_returns_true() {
        workers::block_on(async {
            let collection = CollectionVersion::new(
                "User",
                "v1",
                "coll-user",
                vec![FieldDescription::new(
                    "1",
                    "_docID",
                    FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                )],
            );
            let truncator = Arc::new(CapturingTruncator::default());
            let runner = QueryRunner::new(MockFetcher::new(), vec![collection])
                .with_mutator(Arc::new(CapturingMutator::new()))
                .with_collection_truncator(truncator.clone());

            let result = runner
                .execute_mutation(r#"mutation { truncate_User(filter: {_docID: {_eq: "bae-1"}}) }"#)
                .await
                .unwrap();

            assert_eq!(result, serde_json::json!({"truncate_User": true}));
            let calls = drain(&truncator.calls);
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].0, "User");
            assert!(calls[0].1);
        })
        .unwrap()
    }

    #[test]
    fn filtered_truncate_rejects_transactions_and_sibling_mutations() {
        workers::block_on(async {
            let collection = CollectionVersion::new(
                "User",
                "v1",
                "coll-user",
                vec![FieldDescription::new(
                    "1",
                    "_docID",
                    FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                )],
            );
            let mutator = Arc::new(CapturingMutator::new());
            let runner = QueryRunner::new(MockFetcher::new(), vec![collection])
                .with_mutator(mutator.clone())
                .with_collection_truncator(Arc::new(CapturingTruncator::default()));

            let mixed = runner
                .execute_mutation(
                    r#"mutation {
                    truncate_User
                    delete_User { _docID }
                }"#,
                )
                .await
                .unwrap_err();
            assert!(mixed.to_string().contains("only field in an operation"));

            let mutations = crate::parse_mutations(r#"mutation { truncate_User }"#).unwrap();
            let transaction = runner
                .execute_parsed_mutations(
                    mutations,
                    mutator,
                    None,
                    Some(Arc::new(MockFetcher::new())),
                    None,
                )
                .await
                .unwrap_err();
            assert!(transaction
                .to_string()
                .contains("cannot run in a transaction"));
        })
        .unwrap()
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl crate::mutator::DocMutator for CapturingMutator {
        fn crdts(&self) -> crdt::Crdts {
            crdt::Crdts::default()
        }

        async fn prepare_write(
            &self,
            _collection_name: &str,
            doc: Document,
            _modified_fields: Option<RapidHashSet<String>>,
            _encryption: Option<defra_core::encryption::EncryptionConfig>,
        ) -> Result<Document> {
            self.prepared_writes.fetch_add(1, Ordering::SeqCst);
            Ok(doc)
        }

        async fn create(&self, _collection_name: &str, mut doc: Document) -> Result<CreateResult> {
            self.broadcast_creators
                .push(defra_core::signing::get_broadcast_creator_did());
            if doc.id().is_none() {
                // Stand in for the genesis-CID identity a real create would
                // derive: seed a valid DocID from the capture order.
                let seq = self.created_docs.len();
                doc.set_id(document::DocID::new_v0_from_seed(&format!(
                    "test-doc-{seq}"
                )));
            }
            self.created_docs.push(doc.clone());
            Ok(CreateResult::new(doc.id().unwrap().clone(), doc))
        }

        async fn update(
            &self,
            _collection_name: &str,
            doc: Document,
            modified_fields: RapidHashSet<String>,
        ) -> Result<UpdateResult> {
            Ok(UpdateResult::new(doc, modified_fields.len()))
        }

        async fn delete(&self, _collection_name: &str, doc_id: &DocID) -> Result<DeleteResult> {
            Ok(DeleteResult::new(doc_id.clone(), true))
        }

        async fn exists(&self, _collection_name: &str, _doc_id: &DocID) -> Result<bool> {
            Ok(false)
        }

        async fn get_for_update(
            &self,
            _collection_name: &str,
            _doc_id: &DocID,
        ) -> Result<Option<Document>> {
            Ok(None)
        }
    }

    #[test]
    fn update_preparation_keeps_custom_hook_and_only_records_found_targets() {
        workers::block_on(async {
        let collection = CollectionVersion::new(
            "User",
            "v1",
            "coll-user",
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id(), schema::CType::LwwRegister),
                FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
            ],
        );
        let fetcher = MockFetcher::new();
        let mut existing = crate::test_utils::doc_from_json(r#"{"name":"before"}"#);
        existing.set_id(DocID::new_v0_from_seed("preparation-existing"));
        let existing_id = existing.id().unwrap().to_string();
        let missing_id = "missing-before-preparation";
        fetcher.add_doc("User", existing);
        let mutator = Arc::new(CapturingMutator::new());
        // This custom mutator has the trait defaults: it does not require
        // external write preparation and does not opt in to projected reads.
        assert!(!mutator.requires_write_preparation());
        assert!(!mutator.supports_projected_updates());
        let runner = QueryRunner::new(fetcher, vec![collection]).with_mutator(mutator.clone());
        let request = QueryRequest::new(format!(
            r#"mutation {{ update_User(docIDs: ["{existing_id}", "{missing_id}"], input: {{name: "after"}}) {{ _docID }} }}"#
        ));

        let prepared = runner.prepare_mutations(&request).await.unwrap().unwrap();
        let entry = prepared.mutations.values().next().unwrap();
        assert_eq!(entry.update_targets.len(), 1);
        assert!(entry.update_targets.contains(&existing_id));
        assert!(!entry.update_targets.contains(missing_id));
        assert_eq!(mutator.prepared_writes.load(Ordering::SeqCst), 1);
    })
    .unwrap()
    }

    #[test]
    fn query_executor_mutation_scopes_broadcast_creator_did() {
        workers::block_on(async {
            let collection = CollectionVersion::new(
                "User",
                "v1",
                "coll-user",
                vec![
                    FieldDescription::new(
                        "1",
                        "_docID",
                        FieldKind::doc_id(),
                        schema::CType::LwwRegister,
                    ),
                    FieldDescription::new(
                        "2",
                        "name",
                        FieldKind::string(),
                        schema::CType::LwwRegister,
                    ),
                ],
            );
            let mutator = Arc::new(CapturingMutator::new());
            let runner = QueryRunner::new(MockFetcher::new(), vec![collection])
                .with_mutator(mutator.clone());
            let did =
                identity::Did::new("did:key:z6MktwupdmLXVVqTzCw4i46r4uGyosGXRnR3XjN4Zq7oMMsw")
                    .unwrap();

            let response = runner
                .execute(
                    QueryRequest::new(
                        r#"mutation { add_User(input: [{ name: "Alice" }]) { _docID } }"#,
                    )
                    .with_identity(Some(did.clone())),
                )
                .await;

            assert!(
                !response.has_errors(),
                "unexpected errors: {:?}",
                response.errors
            );
            assert_eq!(mutator.broadcast_creators(), vec![Some(did.to_string())]);
            assert_eq!(defra_core::signing::get_broadcast_creator_did(), None);
        })
        .unwrap()
    }

    // =========================================================================
    // RAII guard tests for #757
    //
    // These tests verify that the EncryptionConfigGuard and
    // BroadcastCreatorDidGuard always clear their respective thread-locals
    // on Drop, regardless of how the owning function exits. They are NOT
    // async tests because the entire point is to exercise thread-local
    // semantics on a single thread synchronously.
    // =========================================================================

    fn make_test_encryption_config() -> defra_core::encryption::EncryptionConfig {
        defra_core::encryption::EncryptionConfig {
            encrypt_doc: true,
            encrypt_fields: vec![],
        }
    }

    #[test]
    fn encryption_config_guard_clears_on_normal_drop() {
        // Pre-condition: ensure clean state
        defra_core::encryption::set_encryption_config(None);

        // Set state and bind guard
        defra_core::encryption::set_encryption_config(Some(make_test_encryption_config()));
        assert!(defra_core::encryption::get_encryption_config().is_some());

        {
            let _guard = EncryptionConfigGuard;
            // Inside the guard scope the state is still set.
            assert!(defra_core::encryption::get_encryption_config().is_some());
            // Guard drops at end of scope.
        }

        // After guard drop the thread-local is cleared.
        assert!(defra_core::encryption::get_encryption_config().is_none());
    }

    #[test]
    fn encryption_config_guard_clears_on_panic_unwind() {
        defra_core::encryption::set_encryption_config(None);
        defra_core::encryption::set_encryption_config(Some(make_test_encryption_config()));

        // Drive the guard's drop via a panic in a catch_unwind boundary,
        // simulating an early-return from a panicking mutation.
        let _ = std::panic::catch_unwind(|| {
            let _guard = EncryptionConfigGuard;
            panic!("simulated mutation panic");
        });

        // Even after a panic unwind, the guard cleared the thread-local.
        assert!(defra_core::encryption::get_encryption_config().is_none());
    }

    #[test]
    fn encryption_config_guard_self_heals_from_leaked_state() {
        // Simulate a previous task that leaked stale state on this thread.
        defra_core::encryption::set_encryption_config(Some(make_test_encryption_config()));
        assert!(defra_core::encryption::get_encryption_config().is_some());

        // Even if the guard is bound without us setting fresh state, on
        // Drop it clears the thread-local — self-healing.
        {
            let _guard = EncryptionConfigGuard;
        }

        assert!(defra_core::encryption::get_encryption_config().is_none());
    }

    #[test]
    fn create_mutation_maps_relation_doc_ids_to_fk_fields() {
        workers::block_on(async {
            let company_collection = CollectionVersion::new(
                "Company",
                "v1",
                "coll-company",
                vec![
                    FieldDescription::new(
                        "1",
                        "_docID",
                        FieldKind::doc_id(),
                        schema::CType::LwwRegister,
                    ),
                    FieldDescription::new(
                        "2",
                        "name",
                        FieldKind::string(),
                        schema::CType::LwwRegister,
                    ),
                    FieldDescription::new(
                        "3",
                        "employees",
                        FieldKind::relation("Employee", true),
                        schema::CType::LwwRegister,
                    )
                    .with_relation_name("employee_company"),
                ],
            );

            let employee_collection = CollectionVersion::new(
                "Employee",
                "v1",
                "coll-employee",
                vec![
                    FieldDescription::new(
                        "1",
                        "_docID",
                        FieldKind::doc_id(),
                        schema::CType::LwwRegister,
                    ),
                    FieldDescription::new(
                        "2",
                        "name",
                        FieldKind::string(),
                        schema::CType::LwwRegister,
                    ),
                    FieldDescription::new(
                        "3",
                        "company",
                        FieldKind::relation("Company", false),
                        schema::CType::LwwRegister,
                    )
                    .with_relation_name("employee_company")
                    .as_primary(),
                    FieldDescription::new(
                        "4",
                        "_companyID",
                        FieldKind::doc_id(),
                        schema::CType::LwwRegister,
                    )
                    .with_relation_name("employee_company")
                    .as_primary(),
                ],
            );

            let fetcher = MockFetcher::new();
            let mutator = Arc::new(CapturingMutator::new());
            let runner = QueryRunner::new(fetcher, vec![company_collection, employee_collection])
                .with_mutator(mutator.clone());

            let request = QueryRequest::new(
                r#"
            mutation {
                add_Employee(input: [{
                    name: "PubEmp in PubCompany"
                    company: "bae-7b649bba-3168-5c05-827c-514c0f8d56fd"
                }]) {
                    _docID
                }
            }
            "#,
            );

            let response = runner.execute(request).await;
            assert!(
                response.errors.is_empty(),
                "unexpected executor errors: {:?}",
                response.errors
            );
            assert!(response.data.is_some());

            // Also exercise the direct mutation path used by internal callers.
            runner
                .execute_mutation(
                    r#"
                mutation {
                    add_Employee(input: [{
                        name: "PubEmp in PubCompany 2"
                        company: "bae-7b649bba-3168-5c05-827c-514c0f8d56fd"
                    }]) {
                        _docID
                    }
                }
                "#,
                )
                .await
                .unwrap();

            let created = mutator.created_docs();
            assert_eq!(created.len(), 2);
            let created_doc = &created[0];
            assert_eq!(
                created_doc
                    .get("_companyID")
                    .and_then(|value| value.as_str()),
                Some("bae-7b649bba-3168-5c05-827c-514c0f8d56fd")
            );
            assert!(created_doc.get("company").is_none());

            let created_doc = &created[1];
            assert_eq!(
                created_doc
                    .get("_companyID")
                    .and_then(|value| value.as_str()),
                Some("bae-7b649bba-3168-5c05-827c-514c0f8d56fd")
            );
            assert!(created_doc.get("company").is_none());
        })
        .unwrap()
    }
}
