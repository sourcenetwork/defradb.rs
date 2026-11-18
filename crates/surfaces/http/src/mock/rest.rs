//! Mock REST operations for testing collection and document handlers.

use async_trait::async_trait;
use identity::Did;
use kovan::Atom;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;

use query::rest::{RestError, RestOperations, RestResult};

/// Internal document storage for mock REST operations.
#[derive(Debug, Clone, Default)]
struct MockDocument {
    doc_id: String,
    data: JsonValue,
}

/// A collection's documents, lock-free in their own right so a per-key update
/// never round-trips through the map.
type Documents = Arc<Atom<Vec<MockDocument>>>;

/// Mock REST operations for testing collection and document handlers.
pub struct MockRestOperations {
    /// Collections with their documents.
    collections: Arc<HopscotchMap<String, Documents, RandomState>>,
}

impl Clone for MockRestOperations {
    fn clone(&self) -> Self {
        Self {
            collections: Arc::clone(&self.collections),
        }
    }
}

impl Default for MockRestOperations {
    fn default() -> Self {
        Self::new()
    }
}

impl MockRestOperations {
    /// Create a new mock REST operations instance with default collections.
    pub fn new() -> Self {
        let collections = HopscotchMap::with_hasher(RandomState::default());

        // Add default Users collection with sample data
        collections.insert(
            "Users".to_string(),
            Arc::new(Atom::new(vec![
                MockDocument {
                    doc_id: "bae-123".to_string(),
                    data: json!({"name": "Alice", "age": 30}),
                },
                MockDocument {
                    doc_id: "bae-456".to_string(),
                    data: json!({"name": "Bob", "age": 25}),
                },
            ])),
        );

        // Add empty Books collection
        collections.insert("Books".to_string(), Arc::new(Atom::new(vec![])));

        Self {
            collections: Arc::new(collections),
        }
    }

    /// Create an empty mock REST operations instance.
    pub fn empty() -> Self {
        Self {
            collections: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
        }
    }

    /// Add a collection (for test setup).
    pub fn with_collection(self, name: &str) -> Self {
        self.collections
            .insert(name.to_string(), Arc::new(Atom::new(vec![])));
        self
    }

    fn documents(&self, collection: &str) -> RestResult<Documents> {
        self.collections
            .get(collection)
            .ok_or_else(|| RestError::collection_not_found(collection))
    }
}

#[async_trait]
impl RestOperations for MockRestOperations {
    async fn list_collections(&self) -> RestResult<Vec<String>> {
        let mut names: Vec<String> = self.collections.keys().collect();
        names.sort();
        Ok(names)
    }

    async fn get_collection_doc_ids(
        &self,
        collection: &str,
        _identity: Option<&Did>,
    ) -> RestResult<Vec<String>> {
        Ok(self
            .documents(collection)?
            .peek(|docs| docs.iter().map(|d| d.doc_id.clone()).collect()))
    }

    async fn get_document(
        &self,
        collection: &str,
        doc_id: &str,
        _identity: Option<&Did>,
    ) -> RestResult<Option<JsonValue>> {
        Ok(self.documents(collection)?.peek(|docs| {
            docs.iter().find(|d| d.doc_id == doc_id).map(|d| {
                let mut result = d.data.clone();
                if let Some(obj) = result.as_object_mut() {
                    obj.insert("_docID".to_string(), json!(d.doc_id));
                }
                result
            })
        }))
    }
}

/// Mock REST operations that always fails (for error path testing).
///
/// Supports configurable error types for testing different error paths.
#[derive(Debug, Clone)]
pub struct FailingMockRestOperations {
    error: RestError,
}

impl FailingMockRestOperations {
    /// Create a mock that always returns an internal error with the given message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            error: RestError::internal(message),
        }
    }

    /// Create a mock that always returns the specified error.
    pub fn with_error(error: RestError) -> Self {
        Self { error }
    }

    /// Create a mock that returns InvalidDocId errors.
    pub fn with_invalid_doc_id(id: impl Into<String>) -> Self {
        Self {
            error: RestError::invalid_doc_id(id),
        }
    }

    /// Create a mock that returns InvalidInput errors.
    pub fn with_invalid_input(msg: impl Into<String>) -> Self {
        Self {
            error: RestError::invalid_input(msg),
        }
    }

    /// Create a mock that returns PermissionDenied errors.
    pub fn with_permission_denied(msg: impl Into<String>) -> Self {
        Self {
            error: RestError::permission_denied(msg),
        }
    }
}

#[async_trait]
impl RestOperations for FailingMockRestOperations {
    async fn list_collections(&self) -> RestResult<Vec<String>> {
        Err(self.error.clone())
    }

    async fn get_collection_doc_ids(
        &self,
        _collection: &str,
        _identity: Option<&Did>,
    ) -> RestResult<Vec<String>> {
        Err(self.error.clone())
    }

    async fn get_document(
        &self,
        _collection: &str,
        _doc_id: &str,
        _identity: Option<&Did>,
    ) -> RestResult<Option<JsonValue>> {
        Err(self.error.clone())
    }
}
