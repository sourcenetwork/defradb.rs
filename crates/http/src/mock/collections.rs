//! Mock collection management operations for testing.

use async_trait::async_trait;
use kovan::{Atom, AtomOption};
use kovan_queue::seg_queue::SegQueue;
use serde_json::json;
use std::sync::Arc;

use crate::mock::update_vec;
use crate::router::{CollectionManagementOperations, CollectionVersionOperations};

type TruncateCall = (String, Option<serde_json::Value>);

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
    last_migration: Arc<AtomOption<lens::LensConfig>>,
    /// Names passed to `delete_collection`, so a test can tell a filtered
    /// document delete from a collection drop. A no-op mock cannot: it lets a
    /// route wired back to the drop keep every assertion green.
    dropped_collections: Arc<Atom<Vec<String>>>,
    truncated_collections: Arc<SegQueue<TruncateCall>>,
}

impl MockCollectionManagementOperations {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn last_migration(&self) -> Option<lens::LensConfig> {
        self.last_migration.load().map(|config| (*config).clone())
    }

    /// Collections dropped through `delete_collection`.
    pub fn dropped_collections(&self) -> Vec<String> {
        self.dropped_collections.load_clone()
    }

    /// Drains the recorded truncate calls, oldest first.
    pub fn truncated_collections(&self) -> Vec<TruncateCall> {
        let mut calls = Vec::new();
        while let Some(call) = self.truncated_collections.pop() {
            calls.push(call);
        }
        calls
    }
}

#[async_trait]
impl CollectionVersionOperations for MockCollectionManagementOperations {
    async fn get_all_collections(&self) -> Result<Vec<schema::CollectionVersion>, String> {
        Ok(vec![mock_collection_version("MockCollection")])
    }
}

#[async_trait]
impl CollectionManagementOperations for MockCollectionManagementOperations {
    async fn list_actions(&self) -> Result<Vec<defra_core::ActionExecution>, String> {
        Ok(Vec::new())
    }

    async fn patch_collection(
        &self,
        collection_name: &str,
        _patch: &str,
        migration: Option<lens::LensConfig>,
    ) -> Result<serde_json::Value, String> {
        match migration {
            Some(migration) => self.last_migration.store_some(migration),
            None => self.last_migration.store_none(),
        }
        Ok(json!({"name": collection_name, "version": "v2"}))
    }

    async fn set_active_version(&self, _version_id: &str) -> Result<(), String> {
        Ok(())
    }

    async fn truncate_collection(
        &self,
        name: &str,
        filter: Option<serde_json::Value>,
    ) -> Result<(), String> {
        self.truncated_collections.push((name.to_string(), filter));
        Ok(())
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

    async fn get_all_collections(&self) -> Result<Vec<schema::CollectionVersion>, String> {
        CollectionVersionOperations::get_all_collections(self).await
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
