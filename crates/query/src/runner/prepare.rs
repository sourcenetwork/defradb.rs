use std::sync::Arc;

use chrono::{FixedOffset, Utc};
use defra_core::encryption::EncryptionConfig;
use rapidhash::{HashMapExt, RapidHashMap};

use crate::error::Result;
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
        let Some(mutator) = self
            .mutator
            .as_ref()
            .filter(|m| m.requires_write_preparation())
        else {
            return Ok(None);
        };
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
        };
        let mut documents = Vec::new();
        let mut projected_ids = rapidhash::RapidHashSet::default();
        for mutation in mutations {
            if !matches!(
                mutation.mutation_type,
                MutationType::Create
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
                .authorize_mutation(mutation, &identity, &fetcher, Some(&projected_ids))
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
            let config = (mutation.encrypt_doc || !mutation.encrypt_fields.is_empty()).then(|| {
                EncryptionConfig {
                    encrypt_doc: mutation.encrypt_doc,
                    encrypt_fields: mutation.encrypt_fields.clone(),
                }
            });
            if mutation.mutation_type == MutationType::Create {
                for input in self.build_create_inputs(mutation, collection)? {
                    let doc =
                        input.to_document_with_schema_and_time(collection, Some(request_time))?;
                    entry.creates.push(
                        mutator
                            .prepare_write(&mutation.collection_name, doc, None, config.clone())
                            .await?,
                    );
                }
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
                let mut docs = fetcher
                    .get_by_ids(&mutation.collection_name, &ids)
                    .await?
                    .into_docs();
                if mutation.mutation_type == MutationType::Upsert && docs.is_empty() {
                    if let Some(input) = mutation.create_input.first() {
                        let input = self.build_upsert_input_from_map(collection, input)?;
                        let doc = input
                            .to_create_input()
                            .to_document_with_schema_and_time(collection, Some(request_time))?;
                        entry.creates.push(
                            mutator
                                .prepare_write(&mutation.collection_name, doc, None, config.clone())
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
                        let modified = if mutation.mutation_type == MutationType::Update {
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
                            .prepare_write(
                                &mutation.collection_name,
                                doc.clone(),
                                Some(modified),
                                config.clone(),
                            )
                            .await?;
                        if let (Some(id), Some(write)) = (doc.id(), doc.write_preparation()) {
                            entry.updates.insert(id.to_string(), Arc::clone(write));
                        }
                        if let Some(id) = doc.id().map(ToString::to_string) {
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
        Ok(Some(Arc::new(prepared)))
    }
}
