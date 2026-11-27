//! Transaction commands.

use serde_json::Value;

use super::DefraClient;
use crate::error::{Result, WrapErr};

impl DefraClient {
    // ---- Transaction operations ----

    /// Create a transaction via `client tx create`.
    pub fn tx_create(&self) -> Result<String> {
        let out = self.exec(&["client", "tx", "new"])?;
        Self::parse_tx_id(&out)
    }

    /// Create a concurrent transaction via `client tx new --concurrent`.
    pub fn tx_create_concurrent(&self) -> Result<String> {
        let out = self.exec(&["client", "tx", "new", "--concurrent"])?;
        Self::parse_tx_id(&out)
    }

    pub(super) fn parse_collection_list(output: &str) -> Result<Vec<String>> {
        let trimmed = output.trim();
        if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
            if let Some(arr) = val.as_array() {
                return Ok(arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect());
            }
        }
        Ok(trimmed
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    pub(super) fn parse_tx_id(output: &str) -> Result<String> {
        let trimmed = output.trim();
        let val: Value =
            serde_json::from_str(trimmed).wrap_err("failed to parse tx create output as JSON")?;
        let id = val
            .get("id")
            .wrap_err("tx create output missing 'id' field")?;
        Ok(id.to_string())
    }

    /// Commit a transaction via `client tx commit <id>`.
    pub fn tx_commit(&self, tx_id: &str) -> Result<String> {
        self.exec(&["client", "tx", "commit", tx_id])
    }

    /// Discard a transaction via `client tx discard <id>`.
    pub fn tx_discard(&self, tx_id: &str) -> Result<String> {
        self.exec(&["client", "tx", "discard", tx_id])
    }

    /// Execute a GraphQL query inside a transaction.
    pub fn query_with_tx(&self, gql: &str, tx_id: &str) -> Result<Value> {
        let out = self.exec(&["client", "--tx", tx_id, "query", gql])?;
        self.parse_query_output(&out)
    }
}
