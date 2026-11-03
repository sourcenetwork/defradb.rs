//! AddNode for creating new documents
//!
//! This node creates documents in storage during query execution, following
//! the Go DefraDB pattern where persistence happens within the plan node.

use rapidhash::{HashMapExt, RapidHashMap};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset};
use defra_core::RequestContext;
use document::Document;
use schema::CollectionVersion;
use serde_json::Value as JsonValue;

use crate::document::DocumentMapping;
use crate::error::{QueryError, Result};
use crate::mutator::{CreateResult, DocMutator};
use crate::planner::{Doc, PlanNode};

use super::create_conversions::{json_to_normal_value_with_kind_and_time, normal_value_to_json};

/// Input for a create mutation - field values to set on the new document.
#[derive(Debug, Clone)]
pub struct AddInput {
    /// Field values keyed by field name
    pub fields: RapidHashMap<String, JsonValue>,
}

impl AddInput {
    /// Create a new empty input.
    pub fn new() -> Self {
        Self {
            fields: RapidHashMap::new(),
        }
    }

    /// Add a field value.
    pub fn with_field(mut self, name: impl Into<String>, value: JsonValue) -> Self {
        self.fields.insert(name.into(), value);
        self
    }

    /// Convert to a Document for storage with schema-aware type coercion.
    ///
    /// This method uses the collection schema to properly coerce values,
    /// such as parsing RFC 3339 strings as DateTime values when the field
    /// type is DateTime (matching Go DefraDB behavior). It also preserves
    /// the CRDT type from the schema (e.g., PnCounter, PCounter).
    pub fn to_document_with_schema(&self, collection: &CollectionVersion) -> Result<Document> {
        self.to_document_with_schema_and_time(collection, None)
    }

    /// Convert to a Document for storage with schema-aware type coercion
    /// and an optional pre-computed request time for UTC_NOW resolution.
    pub fn to_document_with_schema_and_time(
        &self,
        collection: &CollectionVersion,
        request_time: Option<DateTime<FixedOffset>>,
    ) -> Result<Document> {
        // The document carries its collection: Go DefraDB includes the
        // collection_id in the docID hash, so we must too.
        let mut doc = Document::new(collection.clone());

        for (field_name, value) in &self.fields {
            // Look up the field in the schema to get its kind and CRDT type
            let field_def = collection.fields.iter().find(|f| f.name == *field_name);
            let field_kind = field_def.map(|f| &f.kind);

            // Convert JsonValue to appropriate NormalValue, using schema for type coercion
            let normal_value =
                json_to_normal_value_with_kind_and_time(value, field_kind, request_time)?;

            doc.set(field_name.clone(), normal_value).map_err(|e| {
                QueryError::execution(format!("Failed to set field '{}': {}", field_name, e))
            })?;
        }

        // Apply schema defaults for fields not present in the input.
        // Go DefraDB applies @default directive values during document creation
        // for any field not explicitly provided (but not for fields set to null).
        for field_def in &collection.fields {
            // Skip fields already in the input
            if self.fields.contains_key(&field_def.name) {
                continue;
            }

            // Skip fields without a default value
            let default_value = match &field_def.default_value {
                Some(v) => v,
                None => continue,
            };

            let field_kind = Some(&field_def.kind);

            // Convert the default value using schema-aware coercion.
            // This handles UTC_NOW for DateTime fields via json_to_normal_value_with_kind_and_time.
            let normal_value =
                json_to_normal_value_with_kind_and_time(default_value, field_kind, request_time)?;

            doc.set(field_def.name.clone(), normal_value).map_err(|e| {
                QueryError::execution(format!(
                    "Failed to set default value for field '{}': {}",
                    field_def.name, e
                ))
            })?;
        }

        Ok(doc)
    }
}

impl Default for AddInput {
    fn default() -> Self {
        Self::new()
    }
}

/// AddNode creates new documents in a collection.
///
/// This node implements the Volcano iterator model, yielding created documents
/// one at a time. On the first call to `next()`, all documents are created in
/// storage via the `DocMutator`. Subsequent calls iterate through the results.
///
/// # Example
///
/// ```ignore
/// let input = AddInput::new()
///     .with_field("name", json!("Alice"))
///     .with_field("age", json!(30));
///
/// let mut node = AddNode::new("Users", mutator, mapping)
///     .with_input(input);
///
/// node.init().await?;
/// node.start().await?;
///
/// while node.next().await? {
///     let created_doc = node.value();
///     println!("Created: {:?}", created_doc.doc_id());
/// }
/// ```
pub struct AddNode {
    /// Collection name to create documents in
    collection_name: String,
    /// Document mutator for storage operations
    mutator: Arc<dyn DocMutator>,
    /// The context of the request the creates serve: [`RequestContext::ambient`]
    /// until [`AddNode::with_context`] sets it
    context: RequestContext,
    /// Document mapping for field positions
    document_mapping: DocumentMapping,
    /// Collection schema for schema-aware type coercion (e.g., DateTime parsing)
    collection: Option<Arc<CollectionVersion>>,
    /// Pre-computed request time for UTC_NOW resolution (ensures consistency within a request)
    request_time: Option<DateTime<FixedOffset>>,
    /// Input documents to create
    inputs: Vec<AddInput>,
    prepared_documents: Option<Vec<Document>>,
    /// Created documents (populated after first next())
    created_docs: Vec<Doc>,
    /// Current position in created_docs
    position: usize,
    /// Current document being yielded
    current_doc: Doc,
    /// Whether documents have been created yet
    did_create: bool,
    /// Whether the node has been initialized
    initialized: bool,
}

impl AddNode {
    /// Create a new create node for a collection.
    ///
    /// # Arguments
    ///
    /// * `collection_name` - Name of the collection to create documents in
    /// * `mutator` - Document mutator for storage operations
    /// * `document_mapping` - Field mapping for result documents
    pub fn new(
        collection_name: impl Into<String>,
        mutator: Arc<dyn DocMutator>,
        document_mapping: DocumentMapping,
    ) -> Self {
        Self {
            collection_name: collection_name.into(),
            mutator,
            context: RequestContext::ambient(),
            document_mapping,
            collection: None,
            request_time: None,
            inputs: Vec::new(),
            prepared_documents: None,
            created_docs: Vec::new(),
            position: 0,
            current_doc: Doc::default(),
            did_create: false,
            initialized: false,
        }
    }

    /// Serve the request `context` belongs to.
    pub fn with_context(mut self, context: RequestContext) -> Self {
        self.context = context;
        self
    }

    /// Add an input document to create.
    pub fn with_input(mut self, input: AddInput) -> Self {
        self.inputs.push(input);
        self
    }

    /// Add multiple input documents.
    pub fn with_inputs(mut self, inputs: Vec<AddInput>) -> Self {
        self.inputs = inputs;
        self
    }

    pub fn with_prepared_documents(mut self, docs: Option<Vec<Document>>) -> Self {
        self.prepared_documents = docs;
        self
    }

    /// Set the collection schema for schema-aware type coercion.
    ///
    /// When set, the node will use the schema to properly coerce values during
    /// document creation (e.g., parsing RFC 3339 strings as DateTime values).
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

    /// Get the number of documents that were created.
    pub fn created_count(&self) -> usize {
        self.created_docs.len()
    }

    /// Convert a CreateResult to a plan Doc using our document mapping.
    fn create_result_to_doc(&self, result: &CreateResult) -> Result<Doc> {
        let num_fields = self.document_mapping.next_index();
        let mut doc = Doc::new(num_fields);

        // Set document ID at index 0
        doc.set_doc_id(result.doc_id.to_string());

        // Map each field from the created document
        for (field_name, field_value) in result.document.values() {
            if let Some(index) = self.document_mapping.first_index_of_name(field_name) {
                // Convert NormalValue back to JsonValue for the plan Doc
                let json_value = normal_value_to_json(field_value.value());
                doc.set(index, json_value);
            }
        }

        Ok(doc)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl PlanNode for AddNode {
    async fn init(&mut self) -> Result<()> {
        self.position = 0;
        self.created_docs.clear();
        self.did_create = false;
        self.initialized = true;
        Ok(())
    }

    async fn start(&mut self) -> Result<()> {
        Ok(())
    }

    async fn next(&mut self) -> Result<bool> {
        if !self.initialized {
            return Err(QueryError::execution("AddNode.next() called before init()"));
        }

        // On first call, create all documents in a single batch transaction
        if !self.did_create {
            // Convert all inputs to Documents
            let docs = if let Some(docs) = &self.prepared_documents {
                docs.clone()
            } else {
                let mut docs = Vec::with_capacity(self.inputs.len());
                for input in &self.inputs {
                    let collection = self.collection.as_ref().ok_or_else(|| {
                        QueryError::execution("create has no collection definition")
                    })?;
                    docs.push(
                        input.to_document_with_schema_and_time(collection, self.request_time)?,
                    );
                }
                docs
            };

            // Batch create: single transaction, single commit/fsync
            let results = self
                .mutator
                .create_many_as(&self.context, &self.collection_name, docs)
                .await?;

            for result in &results {
                let plan_doc = self.create_result_to_doc(result)?;
                self.created_docs.push(plan_doc);
            }
            self.did_create = true;
        }

        // Iterate through created documents
        if self.position >= self.created_docs.len() {
            return Ok(false);
        }

        self.current_doc = self.created_docs[self.position].deep_clone();
        self.position += 1;
        Ok(true)
    }

    fn value(&self) -> &Doc {
        &self.current_doc
    }

    async fn close(&mut self) -> Result<()> {
        self.created_docs.clear();
        self.initialized = false;
        Ok(())
    }

    fn source(&self) -> Option<&dyn PlanNode> {
        None // AddNode is a leaf node (generates data)
    }

    fn document_map(&self) -> &DocumentMapping {
        &self.document_mapping
    }

    fn kind(&self) -> &'static str {
        "addNode"
    }

    fn explain_inner(&self) -> JsonValue {
        let mut obj = serde_json::Map::new();

        // Convert inputs to JSON array of objects
        let input_array: Vec<JsonValue> = self
            .inputs
            .iter()
            .map(|input| {
                let mut input_obj = serde_json::Map::new();
                for (field_name, value) in &input.fields {
                    input_obj.insert(field_name.clone(), value.clone());
                }
                JsonValue::Object(input_obj)
            })
            .collect();

        obj.insert("input".to_string(), JsonValue::Array(input_array));

        // Include child node (selectTopNode) if present
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
