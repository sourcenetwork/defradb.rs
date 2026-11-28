//! Document commands.

use serde_json::Value;

use super::DefraClient;
use crate::error::{Result, WrapErr};

impl DefraClient {
    // ---- Document operations ----

    /// Create a document via `client document add --collection-name <n> '<json>'`.
    pub fn collection_create(&self, name: &str, doc: &str) -> Result<Value> {
        let out = self.exec(&["client", "document", "add", "--collection-name", name, doc])?;
        let trimmed = out.trim();
        if trimmed.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(trimmed).wrap_err("failed to parse collection_create output")
    }

    /// Get a document via `client document get --collection-name <n> <id>`.
    pub fn collection_get(&self, name: &str, doc_id: &str) -> Result<Value> {
        let out = self.exec(&[
            "client",
            "document",
            "get",
            "--collection-name",
            name,
            doc_id,
        ])?;
        serde_json::from_str(&out).wrap_err("failed to parse collection_get output")
    }

    /// Delete a document via `client document delete --collection-name <n> --docID <id>`.
    pub fn collection_delete(&self, name: &str, doc_id: &str) -> Result<String> {
        self.exec(&[
            "client",
            "document",
            "delete",
            "--collection-name",
            name,
            "--docID",
            doc_id,
        ])
    }

    /// List collections via `client collection list`.
    pub fn collection_list(&self) -> Result<Vec<String>> {
        let out = self.exec(&["client", "collection", "list"])?;
        Self::parse_collection_list(&out)
    }

    /// List document IDs via GraphQL query.
    ///
    /// Go v1.0.0-rc1 removed `client collection docIDs`; both runtimes
    /// now use a GraphQL query to enumerate documents.
    pub fn collection_doc_ids(&self, name: &str) -> Result<Vec<String>> {
        let gql = format!("query {{ {} {{ _docID }} }}", name);
        let data = self.query(&gql)?;
        let mut ids = Vec::new();
        if let Some(arr) = data.get(name).and_then(|v| v.as_array()) {
            for doc in arr {
                if let Some(id) = doc.get("_docID").and_then(|v| v.as_str()) {
                    ids.push(id.to_string());
                }
            }
        }
        Ok(ids)
    }

    /// Truncate a collection via `client collection truncate --name <n>`.
    pub fn collection_truncate(&self, name: &str) -> Result<String> {
        self.exec(&[
            "client",
            "collection",
            "truncate",
            "--collection-name",
            name,
        ])
    }

    /// Update a document via `client document update --collection-name <n> --docID <id> --updater '<json>'`.
    pub fn collection_update(&self, name: &str, doc_id: &str, updater: &str) -> Result<String> {
        self.exec(&[
            "client",
            "document",
            "update",
            "--collection-name",
            name,
            "--docID",
            doc_id,
            "--updater",
            updater,
        ])
    }

    /// Describe a collection via `client collection describe --name <n>`.
    pub fn collection_describe(&self, name: &str) -> Result<Value> {
        let out = self.exec(&[
            "client",
            "collection",
            "describe",
            "--collection-name",
            name,
        ])?;
        serde_json::from_str(&out).wrap_err("failed to parse collection_describe output")
    }
}
