//! Document and node access-control commands.

use serde_json::Value;

use super::DefraClient;
use crate::error::{Result, WrapErr};

impl DefraClient {
    // ---- ACP Document operations ----

    /// Add an ACP policy.
    pub fn acp_policy_add(&self, policy: &str, hex_key: &str) -> Result<Value> {
        let out = self.exec(&[
            "client", "-i", hex_key, "acp", "document", "policy", "add", policy,
        ])?;
        serde_json::from_str(&out).wrap_err("failed to parse acp_policy_add output")
    }

    /// Add an ACP document relationship.
    pub fn acp_relationship_add(
        &self,
        collection: &str,
        doc_id: &str,
        relation: &str,
        actor_did: &str,
        hex_key: &str,
    ) -> Result<Value> {
        let out = self.exec(&[
            "client",
            "-i",
            hex_key,
            "acp",
            "document",
            "relationship",
            "add",
            "-c",
            collection,
            "--docID",
            doc_id,
            "-r",
            relation,
            "-a",
            actor_did,
        ])?;
        serde_json::from_str(&out).wrap_err("failed to parse acp_relationship_add output")
    }

    /// Delete an ACP document relationship.
    pub fn acp_relationship_delete(
        &self,
        collection: &str,
        doc_id: &str,
        relation: &str,
        actor_did: &str,
        hex_key: &str,
    ) -> Result<Value> {
        let out = self.exec(&[
            "client",
            "-i",
            hex_key,
            "acp",
            "document",
            "relationship",
            "delete",
            "-c",
            collection,
            "--docID",
            doc_id,
            "-r",
            relation,
            "-a",
            actor_did,
        ])?;
        serde_json::from_str(&out).wrap_err("failed to parse acp_relationship_delete output")
    }

    // ---- ACP Node (NAC) operations ----

    /// Add an ACP node relationship.
    pub fn acp_node_relationship_add(
        &self,
        relation: &str,
        actor_did: &str,
        hex_key: &str,
    ) -> Result<Value> {
        let out = self.exec(&[
            "client",
            "-i",
            hex_key,
            "acp",
            "node",
            "relationship",
            "add",
            "-r",
            relation,
            "-a",
            actor_did,
        ])?;
        serde_json::from_str(&out).wrap_err("failed to parse acp_node_relationship_add output")
    }

    /// Delete an ACP node relationship.
    pub fn acp_node_relationship_delete(
        &self,
        relation: &str,
        actor_did: &str,
        hex_key: &str,
    ) -> Result<Value> {
        let out = self.exec(&[
            "client",
            "-i",
            hex_key,
            "acp",
            "node",
            "relationship",
            "delete",
            "-r",
            relation,
            "-a",
            actor_did,
        ])?;
        serde_json::from_str(&out).wrap_err("failed to parse acp_node_relationship_delete output")
    }

    /// Get ACP node status.
    pub fn acp_node_status(&self) -> Result<Value> {
        let out = self.exec(&["client", "acp", "node", "status"])?;
        serde_json::from_str(&out).wrap_err("failed to parse acp_node_status output")
    }

    /// Disable ACP node.
    pub fn acp_node_disable(&self) -> Result<Value> {
        let out = self.exec(&["client", "acp", "node", "disable"])?;
        serde_json::from_str(&out).wrap_err("failed to parse acp_node_disable output")
    }

    /// Re-enable ACP node.
    pub fn acp_node_reenable(&self) -> Result<Value> {
        let out = self.exec(&["client", "acp", "node", "re-enable"])?;
        serde_json::from_str(&out).wrap_err("failed to parse acp_node_reenable output")
    }
}
