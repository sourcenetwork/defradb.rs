use std::sync::Arc;

use chrono::{FixedOffset, Utc};
use identity::Did;
use rapidhash::{HashMapExt, RapidHashMap, RapidHashSet};

use crate::error::{QueryError, Result};
use crate::executor::QueryRequest;
use crate::mapper::MutationType;
use crate::prepared::{PreparedMutation, PreparedMutations};
use crate::query_parse::{parse_request_with_limits, validate_parsed_operation, ParsedOperation};
use crate::txn::TransactionRegistry;

use super::{DocFetcher, QueryRunner};

impl<F: DocFetcher + 'static, R: TransactionRegistry> QueryRunner<F, R> {
    pub(super) async fn prepare_mutations(
        &self,
        request: &QueryRequest,
    ) -> Result<Option<Arc<PreparedMutations>>> {
        let Some(mutator) = self.mutator.as_ref() else {
            return Ok(None);
        };
        // Only an operation spelled with the `mutation` keyword is a mutation,
        // so a request without it is left to execution to parse, once.
        if !request.query.contains("mutation") {
            return Ok(None);
        }
        let variables = super::executor::convert_variables(&request.variables);
        let parsed = parse_request_with_limits(
            &request.query,
            variables.as_ref(),
            request.operation_name.as_deref(),
            self.query_limits,
        )?;
        let ParsedOperation::Mutation {
            mutations,
            explain: None,
            ..
        } = &parsed
        else {
            return Ok(None);
        };
        let schema_has_embeddings = if mutator.requires_write_preparation_as(&self.context()) {
            false
        } else {
            let provider = self.effective_provider();
            let mut has_embeddings = false;
            for mutation in mutations {
                if let Some(collection) = provider.get_collection(&mutation.collection_name).await?
                {
                    if !collection.vector_embeddings.is_empty() {
                        has_embeddings = true;
                        break;
                    }
                }
            }
            has_embeddings
        };
        let contains_truncate = mutations
            .iter()
            .any(|mutation| mutation.mutation_type == MutationType::Truncate);
        let request_batch =
            mutations.len() > 1 || self.acp.supports_transactions() || !contains_truncate;
        if !mutator.requires_write_preparation_as(&self.context())
            && !schema_has_embeddings
            && !request_batch
        {
            return Ok(None);
        }
        validate_parsed_operation(&parsed, self.effective_provider().as_ref()).await?;
        if super::executor::check_nac(self, &request.identity, &parsed)
            .await
            .is_some()
        {
            return Ok(None);
        }
        let identity = self.resolve_identity(request.identity.clone());
        let request_time = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let mut prepared = PreparedMutations {
            request_time,
            mutations: RapidHashMap::new(),
            inputs: crate::prepared::PreparedInputMemo::default(),
            engine_managed_retry: !mutations
                .iter()
                .any(|mutation| mutation.mutation_type == MutationType::Truncate)
                && mutator.engine_retries_batches(),
            parsed: None,
        };
        if prepared.engine_managed_retry {
            // The identity the mutation's attempts act as.
            let effective_identity = self
                .acting_identity(identity.as_ref())
                .and_then(|value| Did::new(value).ok())
                .unwrap_or_else(Did::wildcard);
            let mut permissions = rapidhash::RapidHashSet::default();
            for mutation in mutations {
                match mutation.mutation_type {
                    MutationType::Add | MutationType::Update | MutationType::Upsert => {
                        permissions.insert(acp::nac::NodePermission::DocumentUpdate);
                    }
                    MutationType::Delete => {
                        permissions.insert(acp::nac::NodePermission::DocumentDelete);
                    }
                    MutationType::Truncate => {
                        permissions.insert(acp::nac::NodePermission::CollectionTruncate);
                    }
                }
            }
            for permission in permissions {
                let allowed = self
                    .nac
                    .check_permission(&effective_identity, permission)
                    .await;
                let exact_request =
                    serde_json::to_vec(&(effective_identity.to_string(), permission.as_str()))
                        .map_err(|error| QueryError::execution(error.to_string()))?;
                let request = crate::prepared::PreparedInputRequest::new(
                    crate::prepared::PreparedInputKind::NodePermission,
                    exact_request,
                );
                let response = serde_json::to_vec(&allowed)
                    .map_err(|error| QueryError::execution(error.to_string()))?;
                prepared.inputs.insert(request, Arc::<[u8]>::from(response));
            }
        }
        let mut documents = Vec::new();
        let mut projected_ids = rapidhash::RapidHashSet::default();
        for mutation in mutations {
            if !matches!(
                mutation.mutation_type,
                MutationType::Add
                    | MutationType::Update
                    | MutationType::Upsert
                    | MutationType::Delete
            ) {
                continue;
            }
            let fetcher = super::prepare_fetcher::PreparationFetcher {
                base: self.fetcher.as_ref(),
                documents: &documents,
            };
            let authorized = self
                .authorize_mutation(
                    mutation,
                    &identity,
                    &fetcher,
                    Some(&projected_ids),
                    Some(&prepared.inputs),
                    false,
                )
                .await?;
            let collection = &authorized.collection;
            let mut entry = PreparedMutation {
                doc_ids: authorized
                    .acp_filtered_doc_ids
                    .or(authorized.resolved_doc_ids)
                    .or_else(|| mutation.doc_ids.clone()),
                ..Default::default()
            };
            if mutation.mutation_type == MutationType::Delete {
                let ids = match &entry.doc_ids {
                    Some(ids) => ids.clone(),
                    None => fetcher
                        .get_all(&mutation.collection_name)
                        .await?
                        .iter()
                        .filter_map(|doc| doc.id().map(ToString::to_string))
                        .collect(),
                };
                for id in &ids {
                    documents.push((mutation.collection_name.clone(), id.clone(), None));
                }
                entry.doc_ids = Some(ids);
                prepared.mutations.insert(mutation.output_name(), entry);
                continue;
            }
            let context = self
                .context()
                .into_owned()
                .or_encryption(mutation.encryption());
            if mutation.mutation_type == MutationType::Add {
                let docs = self
                    .build_create_inputs(mutation, collection)?
                    .into_iter()
                    .map(|input| {
                        input.to_document_with_schema_and_time(collection, Some(request_time))
                    })
                    .collect::<Result<Vec<_>>>()?;
                entry.creates.extend(
                    mutator
                        .prepare_creates_as(&context, &mutation.collection_name, docs)
                        .await?,
                );
            } else {
                let ids = match entry.doc_ids.as_ref() {
                    Some(ids) => ids.clone(),
                    None if mutation.mutation_type == MutationType::Upsert => Vec::new(),
                    None => fetcher
                        .get_all(&mutation.collection_name)
                        .await?
                        .iter()
                        .filter_map(|doc| doc.id().map(ToString::to_string))
                        .collect(),
                };
                entry.doc_ids = Some(ids.clone());
                // A single unconditional ID update can use the same projected
                // read as execution when no persistent write inputs or
                // embeddings need preparation. Keep the ordinary fetch as a
                // fallback and retain its found-target and prepare_write
                // behavior below.
                let projected_fields = if mutation.mutation_type == MutationType::Update
                    && mutations.len() == 1
                    && mutation.doc_ids.is_some()
                    && mutation.filter.is_none()
                    && mutation.encrypt_fields.is_empty()
                    && !mutation.encrypt_doc
                    && !mutator.requires_write_preparation_as(&self.context())
                    && collection.vector_embeddings.is_empty()
                    && mutator.supports_projected_updates()
                {
                    let patch_fields: RapidHashSet<String> =
                        mutation.update_input.keys().cloned().collect();
                    crate::update_parts::planned(collection, &patch_fields, None)
                } else {
                    None
                };
                let mut docs = if let Some(fields) = projected_fields {
                    match fetcher
                        .get_by_ids_projected(&mutation.collection_name, &ids, &fields)
                        .await?
                    {
                        Some(result) => result,
                        None => fetcher.get_by_ids(&mutation.collection_name, &ids).await?,
                    }
                } else {
                    fetcher.get_by_ids(&mutation.collection_name, &ids).await?
                }
                .into_docs();
                if mutation.mutation_type == MutationType::Upsert && docs.is_empty() {
                    if let Some(input) = mutation.create_input.first() {
                        let input = self.build_upsert_input_from_map(collection, input)?;
                        let doc = input
                            .to_create_input()
                            .to_document_with_schema_and_time(collection, Some(request_time))?;
                        entry.creates.push(
                            mutator
                                .prepare_write_as(&context, &mutation.collection_name, doc, None)
                                .await?,
                        );
                    }
                } else {
                    for doc in &mut docs {
                        if mutation.mutation_type == MutationType::Update {
                            if let Some(filter) = &mutation.filter {
                                if !crate::document::matches_document_filter(
                                    doc, collection, filter,
                                )? {
                                    continue;
                                }
                            }
                        }
                        if let Some(id) = doc.id().map(ToString::to_string) {
                            entry.update_targets.insert(id);
                        }
                        let modified: rapidhash::RapidHashSet<String> = if mutation.mutation_type
                            == MutationType::Update
                        {
                            let input = self.build_update_input(mutation, collection)?;
                            input.apply_to_with_time(doc, Some(collection), Some(request_time))?;
                            input.fields.keys().cloned().collect()
                        } else {
                            let input = self
                                .build_upsert_input_from_map(collection, &mutation.update_input)?;
                            input.apply_to(doc, Some(collection), request_time)?;
                            input.fields.keys().cloned().collect()
                        };
                        let doc = mutator
                            .prepare_write_as(
                                &context,
                                &mutation.collection_name,
                                doc.clone(),
                                Some(modified.clone()),
                            )
                            .await?;
                        if let (Some(id), Some(write)) = (doc.id(), doc.write_preparation()) {
                            entry.updates.insert(id.to_string(), Arc::clone(write));
                        }
                        if let Some(id) = doc.id().map(ToString::to_string) {
                            let mut embeddings = Vec::new();
                            for embedding in &collection.vector_embeddings {
                                if modified.contains(&embedding.field_name)
                                    || !embedding
                                        .fields
                                        .iter()
                                        .any(|source| modified.contains(source))
                                {
                                    continue;
                                }
                                let Some(value) = doc.get(&embedding.field_name).cloned() else {
                                    continue;
                                };
                                let Some(request) = mutator
                                    .prepared_embedding_request(
                                        &mutation.collection_name,
                                        &embedding.field_name,
                                        &doc,
                                    )
                                    .await?
                                else {
                                    continue;
                                };
                                let dependencies = embedding
                                    .fields
                                    .iter()
                                    .filter_map(|field| {
                                        doc.get(field).cloned().map(|value| (field.clone(), value))
                                    })
                                    .collect();
                                embeddings.push(crate::prepared::PreparedEmbedding {
                                    field: embedding.field_name.clone(),
                                    value,
                                    dependencies,
                                    request,
                                });
                            }
                            if !embeddings.is_empty() {
                                entry.embeddings.insert(id.clone(), embeddings);
                            }
                            documents.push((mutation.collection_name.clone(), id, Some(doc)));
                        }
                    }
                }
            }
            for created in &entry.creates {
                if let Some(write) = created.write_preparation() {
                    let mut projected = created.clone();
                    let id = crate::prepared::prepared_doc_id(write);
                    projected_ids.insert(id.to_string());
                    projected.set_id(id);
                    documents.push((
                        mutation.collection_name.clone(),
                        projected.id().unwrap().to_string(),
                        Some(projected),
                    ));
                }
            }
            prepared.mutations.insert(mutation.output_name(), entry);
        }
        prepared.parsed = Some(Arc::new(parsed));
        Ok(Some(Arc::new(prepared)))
    }
}
