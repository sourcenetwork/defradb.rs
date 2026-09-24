//! Mock index operations for testing index handlers.

use async_trait::async_trait;
use kovan::Atom;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::mock::update_vec;
use crate::router::{IndexFieldInfo, IndexInfo, IndexOperations};

/// Mock index operations for testing index handlers.
#[derive(Debug)]
pub struct MockIndexOperations {
    indexes: Arc<Atom<Vec<IndexInfo>>>,
    next_id: Arc<AtomicU64>,
}

impl Clone for MockIndexOperations {
    fn clone(&self) -> Self {
        Self {
            indexes: Arc::clone(&self.indexes),
            next_id: Arc::clone(&self.next_id),
        }
    }
}

impl Default for MockIndexOperations {
    fn default() -> Self {
        Self::new()
    }
}

impl MockIndexOperations {
    /// Create a new mock index operations instance.
    pub fn new() -> Self {
        Self {
            indexes: Arc::new(Atom::new(vec![])),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Create with a pre-existing index.
    pub fn with_index(self, collection: &str, name: &str, fields: Vec<&str>, unique: bool) -> Self {
        let fields: Vec<IndexFieldInfo> = fields
            .into_iter()
            .map(|f| IndexFieldInfo {
                name: f.to_string(),
                direction: Some("ASC".to_string()),
            })
            .collect();
        update_vec(&self.indexes, |indexes| {
            indexes.push(IndexInfo {
                kind: None,
                id: 0,
                name: name.to_string(),
                collection: collection.to_string(),
                collection_id: collection.to_string(),
                fields: fields.clone(),
                unique,
            })
        });
        self
    }
}

#[async_trait]
impl IndexOperations for MockIndexOperations {
    async fn create_index(
        &self,
        collection: &str,
        fields: Vec<String>,
        name: Option<&str>,
        unique: bool,
        vector: Option<schema::VectorIndexDescription>,
    ) -> Result<IndexInfo, String> {
        let index_name = match name {
            Some(n) => n.to_string(),
            None => {
                let id = self.next_id.fetch_add(1, Ordering::Relaxed);
                format!("idx_{}_{}", collection.to_lowercase(), id)
            }
        };

        let index = IndexInfo {
            kind: vector.map(schema::IndexKind::Vector),
            id: 0,
            name: index_name,
            collection: collection.to_string(),
            collection_id: collection.to_string(),
            fields: fields
                .into_iter()
                .map(|f| IndexFieldInfo {
                    name: f,
                    direction: Some("ASC".to_string()),
                })
                .collect(),
            unique,
        };

        update_vec(&self.indexes, |indexes| indexes.push(index.clone()));
        Ok(index)
    }

    async fn list_indexes(&self, collection: Option<&str>) -> Result<Vec<IndexInfo>, String> {
        Ok(self.indexes.peek(|indexes| match collection {
            Some(col) => indexes
                .iter()
                .filter(|i| i.collection == col)
                .cloned()
                .collect(),
            None => indexes.clone(),
        }))
    }

    async fn delete_index(&self, collection: &str, name: &str) -> Result<(), String> {
        let removed = update_vec(&self.indexes, |indexes| {
            let initial_len = indexes.len();
            indexes.retain(|i| !(i.collection == collection && i.name == name));
            indexes.len() < initial_len
        });
        if removed {
            Ok(())
        } else {
            Err(format!(
                "index '{}' not found in collection '{}'",
                name, collection
            ))
        }
    }
}

/// Mock index operations that always fails with a configurable error.
#[derive(Debug, Clone)]
pub struct FailingMockIndexOperations {
    error: String,
}

impl FailingMockIndexOperations {
    /// Create a new failing mock with the given error message.
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
        }
    }
}

#[async_trait]
impl IndexOperations for FailingMockIndexOperations {
    async fn create_index(
        &self,
        _collection: &str,
        _fields: Vec<String>,
        _name: Option<&str>,
        _unique: bool,
        _vector: Option<schema::VectorIndexDescription>,
    ) -> Result<IndexInfo, String> {
        Err(self.error.clone())
    }

    async fn list_indexes(&self, _collection: Option<&str>) -> Result<Vec<IndexInfo>, String> {
        Err(self.error.clone())
    }

    async fn delete_index(&self, _collection: &str, _name: &str) -> Result<(), String> {
        Err(self.error.clone())
    }
}
