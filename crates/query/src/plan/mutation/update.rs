//! UpdateNode for updating existing documents
//!
//! This node updates documents in storage during query execution, following
//! the Go DefraDB pattern where persistence happens within the plan node.

use rapidhash::{HashMapExt, RapidHashMap, RapidHashSet};
use std::sync::Arc;

use async_trait::async_trait;
use crdt::Crdts;
use defra_core::RequestContext;
use document::{DocID, Document};
use schema::CollectionVersion;
use serde_json::Value as JsonValue;
use tracing;

use crate::document::{document_to_plan_doc, DocumentMapping};
use crate::error::{QueryError, Result};
use crate::fetcher::DocFetcher;
use crate::mapper::{Filter, InputOp};
use crate::mutator::{DocMutator, UpdateResult};
use crate::planner::{Doc, PlanNode};

use super::create_conversions::{json_to_normal_value_with_kind_and_time, normal_value_to_json};
use super::update_ops;
use chrono::{DateTime, FixedOffset};

fn is_known_prepared_update_target(
    targets: Option<&RapidHashSet<String>>,
    writes: &RapidHashMap<String, Arc<document::WritePreparation>>,
    id: &str,
) -> bool {
    targets.map_or_else(|| writes.contains_key(id), |targets| targets.contains(id))
}

/// Input for an update mutation - field values to patch on existing documents.
#[derive(Debug, Clone)]
pub struct UpdateInput {
    /// Field values to update, keyed by field name
    pub fields: RapidHashMap<String, JsonValue>,
    /// Explicit CRDT ops, keyed by field
    pub ops: RapidHashMap<String, InputOp>,
}

impl UpdateInput {
    /// Create a new empty input.
    pub fn new() -> Self {
        Self {
            fields: RapidHashMap::new(),
            ops: RapidHashMap::new(),
        }
    }

    /// Every field this update writes, through a value or an op.
    pub fn field_names(&self) -> RapidHashSet<String> {
        self.fields.keys().chain(self.ops.keys()).cloned().collect()
    }

    /// Add a field value to update.
    pub fn with_field(mut self, name: impl Into<String>, value: JsonValue) -> Self {
        self.fields.insert(name.into(), value);
        self
    }

    /// Apply this update to a document, using schema-aware type coercion when available.
    ///
    /// For counter CRDT fields (PCounter/PNCounter), the input value is treated as an
    /// increment rather than a replacement, matching Go DefraDB behavior.
    pub fn apply_to(
        &self,
        crdts: &Crdts,
        doc: &mut Document,
        collection: Option<&CollectionVersion>,
    ) -> Result<usize> {
        self.apply_to_with_time(crdts, doc, collection, None)
    }

    /// Apply this update to a document with an optional pre-computed request time for UTC_NOW.
    pub fn apply_to_with_time(
        &self,
        crdts: &Crdts,
        doc: &mut Document,
        collection: Option<&CollectionVersion>,
        request_time: Option<DateTime<FixedOffset>>,
    ) -> Result<usize> {
        let mut modified_count = 0;

        for (field_name, value) in &self.fields {
            let field_def =
                collection.and_then(|c| c.fields.iter().find(|f| f.name == *field_name));
            let normal_value = json_to_normal_value_with_kind_and_time(
                value,
                field_def.map(|f| &f.kind),
                request_time,
            )?;
            let op = update_ops::implicit_op(crdts, field_def, normal_value)?;
            update_ops::write_op(crdts, doc, field_name, field_def, op)?;
            modified_count += 1;
        }

        for (field_name, op) in &self.ops {
            let field_def =
                collection.and_then(|c| c.fields.iter().find(|f| f.name == *field_name));
            let op = update_ops::named_op(crdts, field_name, field_def, op, request_time)?;
            update_ops::write_op(crdts, doc, field_name, field_def, op)?;
            modified_count += 1;
        }

        Ok(modified_count)
    }
}

impl Default for UpdateInput {
    fn default() -> Self {
        Self::new()
    }
}

/// UpdateNode updates existing documents in a collection.
///
/// This node implements the Volcano iterator model. On the first call to `next()`,
/// it finds all matching documents (by docIDs or filter), applies the update patch,
/// and persists them via the `DocMutator`. Subsequent calls iterate through results.
///
/// # Example
///
/// ```ignore
/// let input = UpdateInput::new()
///     .with_field("email", json!("newemail@example.com"));
///
/// let mut node = UpdateNode::new("Users", mutator, fetcher, mapping)
///     .with_doc_ids(vec!["bae-123".to_string()])
///     .with_input(input);
///
/// node.init().await?;
/// node.start().await?;
///
/// while node.next().await? {
///     let updated_doc = node.value();
///     println!("Updated: {:?}", updated_doc.doc_id());
/// }
/// ```
pub struct UpdateNode {
    /// Collection name to update documents in
    collection_name: String,
    /// Document mutator for storage operations
    mutator: Arc<dyn DocMutator>,
    /// The context of the request the updates serve: [`RequestContext::ambient`]
    /// until [`UpdateNode::with_context`] sets it
    context: RequestContext,
    /// Document fetcher for resolving filters and getting all documents
    fetcher: Arc<dyn DocFetcher>,
    /// Document mapping for field positions
    document_mapping: DocumentMapping,
    /// Collection schema for schema-aware type coercion (e.g., DateTime parsing)
    collection: Option<Arc<CollectionVersion>>,
    /// Pre-computed request time for UTC_NOW resolution (ensures consistency within a request)
    request_time: Option<DateTime<FixedOffset>>,
    /// Document IDs to update (mutually exclusive with filter)
    doc_ids: Option<Vec<String>>,
    /// Filter to find documents to update (mutually exclusive with doc_ids)
    filter: Option<Filter>,
    /// Update input (fields to patch)
    input: UpdateInput,
    prepared_writes: Option<RapidHashMap<String, Arc<document::WritePreparation>>>,
    prepared_update_targets: Option<RapidHashSet<String>>,
    prepared_embeddings: Option<RapidHashMap<String, Vec<crate::prepared::PreparedEmbedding>>>,
    prepared_inputs: Option<crate::prepared::PreparedInputMemo>,
    /// Updated documents (populated after first next())
    updated_docs: Vec<Doc>,
    /// Document IDs that were requested but not found
    not_found_ids: Vec<String>,
    /// Current position in updated_docs
    position: usize,
    /// Current document being yielded
    current_doc: Doc,
    /// Whether updates have been performed yet
    did_update: bool,
    /// Whether the node has been initialized
    initialized: bool,
}

impl UpdateNode {
    /// Create a new update node for a collection.
    ///
    /// # Arguments
    ///
    /// * `collection_name` - Name of the collection to update documents in
    /// * `mutator` - Document mutator for storage operations
    /// * `fetcher` - Document fetcher for resolving filters and getting all documents
    /// * `document_mapping` - Field mapping for result documents
    pub fn new(
        collection_name: impl Into<String>,
        mutator: Arc<dyn DocMutator>,
        fetcher: Arc<dyn DocFetcher>,
        document_mapping: DocumentMapping,
    ) -> Self {
        Self {
            collection_name: collection_name.into(),
            mutator,
            context: RequestContext::ambient(),
            fetcher,
            document_mapping,
            collection: None,
            request_time: None,
            doc_ids: None,
            filter: None,
            input: UpdateInput::new(),
            prepared_writes: None,
            prepared_update_targets: None,
            prepared_embeddings: None,
            prepared_inputs: None,
            updated_docs: Vec::new(),
            not_found_ids: Vec::new(),
            position: 0,
            current_doc: Doc::default(),
            did_update: false,
            initialized: false,
        }
    }

    /// Serve the request `context` belongs to.
    pub fn with_context(mut self, context: RequestContext) -> Self {
        self.context = context;
        self
    }

    /// Set specific document IDs to update.
    pub fn with_doc_ids(mut self, doc_ids: Vec<String>) -> Self {
        self.doc_ids = Some(doc_ids);
        self
    }

    /// Set a filter to find documents to update.
    pub fn with_filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Set the update input (fields to patch).
    pub fn with_input(mut self, input: UpdateInput) -> Self {
        self.input = input;
        self
    }

    pub fn with_prepared_writes(
        mut self,
        writes: Option<RapidHashMap<String, Arc<document::WritePreparation>>>,
    ) -> Self {
        self.prepared_writes = writes;
        self
    }

    pub fn with_prepared_update_targets(mut self, targets: Option<RapidHashSet<String>>) -> Self {
        self.prepared_update_targets = targets;
        self
    }

    pub fn with_prepared_embeddings(
        mut self,
        embeddings: Option<RapidHashMap<String, Vec<crate::prepared::PreparedEmbedding>>>,
        inputs: Option<crate::prepared::PreparedInputMemo>,
    ) -> Self {
        self.prepared_embeddings = embeddings;
        self.prepared_inputs = inputs;
        self
    }

    /// Set the collection schema for schema-aware type coercion.
    pub fn with_collection(mut self, collection: Arc<CollectionVersion>) -> Self {
        self.collection = Some(collection);
        self
    }

    /// Set the pre-computed request time for UTC_NOW resolution.
    ///
    /// When set, all `UTC_NOW` values in this node's inputs will resolve
    /// to the same timestamp, matching Go DefraDB's behavior.
    pub fn with_request_time(mut self, request_time: DateTime<FixedOffset>) -> Self {
        self.request_time = Some(request_time);
        self
    }

    /// Get the number of documents that were updated.
    pub fn updated_count(&self) -> usize {
        self.updated_docs.len()
    }

    /// Get the document IDs that were requested but not found.
    ///
    /// This allows callers to detect when update operations silently
    /// skipped documents due to them not existing in the collection.
    pub fn not_found_ids(&self) -> &[String] {
        &self.not_found_ids
    }

    /// Convert an UpdateResult to a plan Doc using our document mapping.
    fn update_result_to_doc(&self, result: &UpdateResult) -> Result<Doc> {
        let num_fields = self.document_mapping.next_index();
        let mut doc = Doc::new(num_fields);

        // Set document ID at index 0
        if let Some(doc_id) = result.document.id() {
            doc.set_doc_id(doc_id.to_string());
        }

        // Map each field from the updated document
        for (field_name, field_value) in result.document.values() {
            if let Some(index) = self.document_mapping.first_index_of_name(field_name) {
                let json_value = normal_value_to_json(field_value.value());
                doc.set(index, json_value);
            }
        }

        Ok(doc)
    }

    /// The parts this update decides on, or `None` when it reads its
    /// documents whole (see [`crate::update_parts`]).
    fn projected_update_fields(&self) -> Option<RapidHashSet<String>> {
        if !self.mutator.supports_projected_updates() {
            return None;
        }
        let collection = self.collection.as_deref()?;
        let patch: RapidHashSet<String> = self.input.field_names();
        self.mutator.update_run().planned(
            &self.mutator.crdts(),
            collection,
            &patch,
            self.filter.as_ref(),
        )
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl PlanNode for UpdateNode {
    async fn init(&mut self) -> Result<()> {
        self.position = 0;
        self.updated_docs.clear();
        self.not_found_ids.clear();
        self.did_update = false;
        self.initialized = true;
        Ok(())
    }

    async fn start(&mut self) -> Result<()> {
        Ok(())
    }

    async fn next(&mut self) -> Result<bool> {
        if !self.initialized {
            return Err(QueryError::execution(
                "UpdateNode.next() called before init()",
            ));
        }

        // On first call, perform all updates
        if !self.did_update {
            let decided_on = self.projected_update_fields();
            // Get document IDs to update
            let doc_ids_to_update = if let Some(ref ids) = self.doc_ids {
                // Explicit doc_ids provided
                ids.clone()
            } else {
                // No doc_ids - fetch documents and optionally filter
                let all_docs = match &decided_on {
                    Some(fields) => {
                        self.fetcher
                            .get_all_projected(&self.collection_name, fields)
                            .await?
                    }
                    None => self.fetcher.get_all(&self.collection_name).await?,
                };
                let mut matching_ids = Vec::new();

                for doc in all_docs {
                    // If filter exists, check if document matches
                    if let Some(ref filter) = self.filter {
                        let plan_doc = document_to_plan_doc(&doc, &self.document_mapping)?;
                        if !filter.matches(plan_doc.fields(), &self.document_mapping)? {
                            continue;
                        }
                    }
                    // Include this document
                    if let Some(id) = doc.id() {
                        matching_ids.push(id.to_string());
                    }
                }
                matching_ids
            };

            // Update each document
            for doc_id_str in doc_ids_to_update {
                if DocID::from_string(&doc_id_str).is_err() {
                    // Invalid DocID format - treat as not found (Go compatibility)
                    tracing::warn!(
                        collection = %self.collection_name,
                        doc_id = %doc_id_str,
                        "Invalid DocID format - skipping"
                    );
                    self.not_found_ids.push(doc_id_str.clone());
                    continue;
                }

                // Fetch document through the lensed fetcher so lens migrations
                // are applied before the update. This matches Go's behavior where
                // the lensedFetcher migrates documents on read, ensuring migrated
                // field values (e.g., SetDefault) are present before the update
                // patch is applied and the result is returned.
                let projected_result = if let Some(fields) = &decided_on {
                    self.fetcher
                        .get_by_ids_projected(
                            &self.collection_name,
                            std::slice::from_ref(&doc_id_str),
                            fields,
                        )
                        .await?
                } else {
                    None
                };
                let (fetch_result, projected_fields) = match projected_result {
                    Some(result) => (result, decided_on.clone()),
                    None => (
                        self.fetcher
                            .get_by_ids(&self.collection_name, std::slice::from_ref(&doc_id_str))
                            .await?,
                        None,
                    ),
                };
                let doc_opt = fetch_result.into_docs().into_iter().next();

                if let Some(mut doc) = doc_opt {
                    // Filter resolution and this point-in-time fetch can be
                    // separated by permission checks and other async work.
                    // Revalidate against the exact snapshot used for the
                    // update so a document that stopped matching is skipped.
                    // Conditions on related documents were decided by the
                    // select that resolved the IDs; only the document's own
                    // conditions can be checked against it here.
                    let local_filter = self.filter.as_ref().and_then(Filter::local_part);
                    if let Some(ref filter) = local_filter {
                        let matches = if let Some(collection) = &self.collection {
                            crate::document::matches_document_filter(&doc, collection, filter)?
                        } else {
                            let plan_doc = document_to_plan_doc(&doc, &self.document_mapping)?;
                            filter.matches(plan_doc.fields(), &self.document_mapping)?
                        };
                        if !matches {
                            continue;
                        }
                    }
                    let expected = doc.clone();

                    // Apply update input with schema-aware coercion and request time
                    self.input.apply_to_with_time(
                        &self.mutator.crdts(),
                        &mut doc,
                        self.collection.as_deref(),
                        self.request_time,
                    )?;

                    // Collect the modified field names for block creation
                    let mut modified_fields: RapidHashSet<String> = self.input.field_names();

                    if let Some(embeddings) = &self.prepared_embeddings {
                        let id = doc.id().map(ToString::to_string).unwrap_or_default();
                        if let Some(prepared) = embeddings.get(&id) {
                            for embedding in prepared {
                                let dependencies_match = embedding
                                    .dependencies
                                    .iter()
                                    .all(|(field, value)| doc.get(field) == Some(value));
                                let Some(request) = self
                                    .mutator
                                    .prepared_embedding_request(
                                        &self.collection_name,
                                        &embedding.field,
                                        &doc,
                                    )
                                    .await?
                                else {
                                    continue;
                                };
                                if dependencies_match && embedding.request == request {
                                    doc.set(embedding.field.clone(), embedding.value.clone())
                                        .map_err(|e| QueryError::execution(e.to_string()))?;
                                    modified_fields.insert(embedding.field.clone());
                                    continue;
                                }

                                let Some(response) = self
                                    .prepared_inputs
                                    .as_ref()
                                    .and_then(|memo| memo.get(&request))
                                else {
                                    if !dependencies_match {
                                        crate::prepared::record_dependency_rerun();
                                    }
                                    return Err(QueryError::NeedsInput(request));
                                };
                                let vector: Vec<f64> =
                                    serde_json::from_slice(&response).map_err(|error| {
                                        QueryError::execution(format!(
                                            "invalid prepared embedding response: {error}"
                                        ))
                                    })?;
                                doc.set(
                                    embedding.field.clone(),
                                    document::NormalValue::Float64Array(vector),
                                )
                                .map_err(|e| QueryError::execution(e.to_string()))?;
                                modified_fields.insert(embedding.field.clone());
                            }
                        }
                    }

                    if let Some(writes) = &self.prepared_writes {
                        let id = doc.id().map(ToString::to_string).unwrap_or_default();
                        let known_target = is_known_prepared_update_target(
                            self.prepared_update_targets.as_ref(),
                            writes,
                            &id,
                        );
                        if !known_target {
                            return Err(QueryError::transaction_conflict(
                                "mutation target changed after key preparation",
                            ));
                        }
                        if let Some(prepared) = writes.get(&id) {
                            doc.set_write_preparation(Arc::clone(prepared));
                        }
                    }

                    // Persist update with modified field tracking
                    let result = if let Some(expected_fields) = projected_fields {
                        self.mutator
                            .update_if_fields_unchanged_as(
                                &self.context,
                                &self.collection_name,
                                expected,
                                expected_fields,
                                doc,
                                modified_fields,
                            )
                            .await?
                    } else {
                        self.mutator
                            .update_if_unchanged_as(
                                &self.context,
                                &self.collection_name,
                                expected,
                                doc,
                                modified_fields,
                            )
                            .await?
                    };

                    // Convert to plan Doc
                    let plan_doc = self.update_result_to_doc(&result)?;

                    // Go does NOT re-filter after update: the filter is only used to
                    // find documents to update, and all updated documents are returned
                    // regardless of whether they still match the filter.
                    self.updated_docs.push(plan_doc);
                } else {
                    // Track and log missing documents instead of silently skipping
                    tracing::warn!(
                        collection = %self.collection_name,
                        doc_id = %doc_id_str,
                        "Document not found for update - skipping"
                    );
                    self.not_found_ids.push(doc_id_str.clone());
                }
            }

            self.did_update = true;
        }

        // Iterate through updated documents
        if self.position >= self.updated_docs.len() {
            return Ok(false);
        }

        self.current_doc = self.updated_docs[self.position].deep_clone();
        self.position += 1;
        Ok(true)
    }

    fn value(&self) -> &Doc {
        &self.current_doc
    }

    async fn close(&mut self) -> Result<()> {
        self.updated_docs.clear();
        self.not_found_ids.clear();
        self.initialized = false;
        Ok(())
    }

    fn source(&self) -> Option<&dyn PlanNode> {
        None // UpdateNode is a leaf node
    }

    fn document_map(&self) -> &DocumentMapping {
        &self.document_mapping
    }

    fn kind(&self) -> &'static str {
        "updateNode"
    }

    fn explain_inner(&self) -> JsonValue {
        let mut obj = serde_json::Map::new();

        // docID: array of doc IDs to update (null if using filter)
        if let Some(ref doc_ids) = self.doc_ids {
            obj.insert(
                "docID".to_string(),
                JsonValue::Array(
                    doc_ids
                        .iter()
                        .map(|id| JsonValue::String(id.clone()))
                        .collect(),
                ),
            );
        } else {
            obj.insert("docID".to_string(), JsonValue::Null);
        }

        // filter: the filter expression (null if using doc IDs)
        if let Some(ref filter) = self.filter {
            obj.insert("filter".to_string(), serde_json::json!(filter.conditions()));
        } else {
            obj.insert("filter".to_string(), JsonValue::Null);
        }

        // input: the update values as a map
        let mut input_obj = serde_json::Map::new();
        for (field_name, value) in &self.input.fields {
            input_obj.insert(field_name.clone(), value.clone());
        }
        for (field_name, op) in &self.input.ops {
            input_obj.insert(
                field_name.clone(),
                serde_json::to_value(op).unwrap_or(JsonValue::Null),
            );
        }
        obj.insert("input".to_string(), JsonValue::Object(input_obj));

        // Include child node if present
        if let Some(source) = self.source() {
            let child_explain = source.explain();
            if let Some(child_obj) = child_explain.as_object() {
                for (key, value) in child_obj {
                    obj.insert(key.clone(), value.clone());
                }
            }
        }

        JsonValue::Object(obj)
    }
}

#[cfg(test)]
mod prepared_target_tests {
    use super::*;
    use crate::{
        mutator::{CreateResult, DeleteResult},
        planner::PlanNode,
        test_utils::MockFetcher,
    };
    use async_trait::async_trait;
    use document::DocID;
    use kovan::Atom;

    #[derive(Default)]
    struct RecordingMutator {
        updates: Atom<Vec<(String, bool)>>,
    }

    #[async_trait]
    impl DocMutator for RecordingMutator {
        fn crdts(&self) -> crdt::Crdts {
            crdt::Crdts::default()
        }

        async fn create(&self, _collection: &str, _doc: Document) -> Result<CreateResult> {
            Err(QueryError::execution("unused in update target test"))
        }

        async fn update(
            &self,
            _collection: &str,
            doc: Document,
            fields: RapidHashSet<String>,
        ) -> Result<UpdateResult> {
            let recorded = (
                doc.id().unwrap().to_string(),
                doc.write_preparation().is_some(),
            );
            self.updates.rcu(|updates| {
                let mut next = updates.clone();
                next.push(recorded.clone());
                next
            });
            Ok(UpdateResult::new(doc, fields.len()))
        }

        async fn delete(&self, _collection: &str, id: &DocID) -> Result<DeleteResult> {
            Ok(DeleteResult::new(id.clone(), false))
        }

        async fn exists(&self, _collection: &str, _id: &DocID) -> Result<bool> {
            Ok(false)
        }

        async fn get_for_update(&self, _collection: &str, _id: &DocID) -> Result<Option<Document>> {
            Ok(None)
        }
    }

    fn doc(seed: &str) -> Document {
        let mut doc = crate::test_utils::doc_from_json(r#"{"value":"before"}"#);
        doc.set_id(DocID::new_v0_from_seed(seed));
        doc
    }

    fn make_node(
        docs: &[Document],
        ids: &[DocID],
        targets: RapidHashSet<String>,
        writes: RapidHashMap<String, Arc<document::WritePreparation>>,
        mutator: Arc<RecordingMutator>,
    ) -> UpdateNode {
        let fetcher = Arc::new(MockFetcher::new());
        for doc in docs {
            fetcher.add_doc("Things", doc.clone());
        }
        let mut mapping = DocumentMapping::new();
        mapping.add(0, "_docID");
        mapping.add(1, "value");
        UpdateNode::new("Things", mutator, fetcher, mapping)
            .with_doc_ids(ids.iter().map(ToString::to_string).collect())
            .with_input(UpdateInput::new().with_field("value", serde_json::json!("after")))
            .with_prepared_writes(Some(writes))
            .with_prepared_update_targets(Some(targets))
    }

    #[test]
    fn update_node_accepts_matched_plain_and_keyed_targets() {
        futures::executor::block_on(async {
            let keyed_doc = doc("keyed-doc");
            let plain_doc = doc("plain-doc");
            let keyed = keyed_doc.id().unwrap().clone();
            let plain = plain_doc.id().unwrap().clone();
            let mut targets = RapidHashSet::default();
            targets.insert(keyed.to_string());
            targets.insert(plain.to_string());
            let mut writes = RapidHashMap::default();
            writes.insert(
                keyed.to_string(),
                Arc::new(document::WritePreparation::new(1, 1)),
            );
            let mutator = Arc::new(RecordingMutator::default());
            let mut node = make_node(
                &[keyed_doc, plain_doc],
                &[keyed.clone(), plain.clone()],
                targets,
                writes,
                Arc::clone(&mutator),
            );
            node.init().await.unwrap();
            assert!(node.next().await.unwrap());
            assert_eq!(node.updated_count(), 2);
            let updates = mutator.updates.load_clone();
            assert!(updates.contains(&(keyed.to_string(), true)));
            assert!(updates.contains(&(plain.to_string(), false)));
        });
    }

    #[test]
    fn update_node_rejects_a_target_absent_from_prepared_target_set() {
        futures::executor::block_on(async {
            let keyed_doc = doc("keyed-doc");
            let plain_doc = doc("plain-doc");
            let appeared_doc = doc("appeared-after-preparation");
            let keyed = keyed_doc.id().unwrap().clone();
            let plain = plain_doc.id().unwrap().clone();
            let appeared = appeared_doc.id().unwrap().clone();
            let mut targets = RapidHashSet::default();
            targets.insert(keyed.to_string());
            targets.insert(plain.to_string());
            let mut writes = RapidHashMap::default();
            writes.insert(
                keyed.to_string(),
                Arc::new(document::WritePreparation::new(1, 1)),
            );
            let mutator = Arc::new(RecordingMutator::default());
            let mut node = make_node(
                &[keyed_doc, plain_doc, appeared_doc],
                &[keyed.clone(), plain.clone(), appeared.clone()],
                targets,
                writes,
                Arc::clone(&mutator),
            );
            node.init().await.unwrap();
            let error = node.next().await.unwrap_err();
            assert!(error
                .to_string()
                .contains("mutation target changed after key preparation"));
            let updates = mutator.updates.load_clone();
            assert_eq!(updates.len(), 2);
            assert!(!updates.iter().any(|(id, _)| id == &appeared.to_string()));
        });
    }
}
