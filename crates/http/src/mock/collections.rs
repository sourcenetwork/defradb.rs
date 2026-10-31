//! Mock collection management operations for testing.

use async_trait::async_trait;
use kovan::Atom;
use serde_json::json;
use std::sync::Arc;

use crate::mock::update_vec;
use crate::router::CollectionManagementOperations;

fn mock_collection_version(name: &str) -> schema::CollectionVersion {
    serde_json::from_value(json!({
        "Name": name,
        "VersionID": "mock-version-id",
        "CollectionID": "mock-collection-id",
    }))
    .expect("mock collection version should deserialize")
}

/// Mock collection management operations for testing.
#[derive(Clone, Default)]
pub struct MockCollectionManagementOperations {
    /// Names passed to `delete_collection`, so a test can tell a filtered
    /// document delete from a collection drop. A no-op mock cannot: it lets a
    /// route wired back to the drop keep every assertion green.
    dropped_collections: Arc<Atom<Vec<String>>>,
}

impl MockCollectionManagementOperations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Collections dropped through `delete_collection`.
    pub fn dropped_collections(&self) -> Vec<String> {
        self.dropped_collections.load_clone()
    }
}

#[async_trait]
impl CollectionManagementOperations for MockCollectionManagementOperations {
    async fn list_actions(&self) -> Result<Vec<defra_core::ActionExecution>, String> {
        Ok(Vec::new())
    }

    async fn purge(&self) -> Result<(), String> {
        Ok(())
    }

    async fn get_collection_by_name(
        &self,
        name: &str,
    ) -> Result<Option<schema::CollectionVersion>, String> {
        Ok(Some(mock_collection_version(name)))
    }

    async fn has_collection(&self, _name: &str) -> Result<bool, String> {
        Ok(true)
    }

    async fn find_collection_by_id(
        &self,
        _collection_id: &str,
    ) -> Result<Option<schema::CollectionVersion>, String> {
        Ok(Some(mock_collection_version("MockCollection")))
    }

    async fn get_collection_by_version_id(
        &self,
        _version_id: &str,
    ) -> Result<Option<schema::CollectionVersion>, String> {
        Ok(Some(mock_collection_version("MockCollection")))
    }

    async fn delete_collection_versions(&self, _version_ids: Vec<String>) -> Result<(), String> {
        Ok(())
    }

    async fn delete_collection(&self, name: &str) -> Result<(), String> {
        update_vec(&self.dropped_collections, |dropped| {
            dropped.push(name.to_string())
        });
        Ok(())
    }

    async fn delete_collections(
        &self,
        _names: Vec<String>,
        _active_only: bool,
    ) -> Result<(), String> {
        Ok(())
    }
}
