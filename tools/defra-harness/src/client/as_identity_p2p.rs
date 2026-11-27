//! P2P commands issued under an identity.

use serde_json::Value;

use super::DefraClient;
use crate::error::{Result, WrapErr};

impl DefraClient {
    // ---- Identity-aware P2P operations ----

    pub fn p2p_connect_with_identity(&self, addrs: &[&str], hex_key: &str) -> Result<String> {
        let mut args = vec!["client", "p2p", "connect"];
        args.extend(addrs);
        self.exec_with_identity(hex_key, &args)
    }

    pub fn p2p_collection_add_with_identity(
        &self,
        collections: &[&str],
        hex_key: &str,
    ) -> Result<String> {
        let cols = collections.join(",");
        self.exec_with_identity(hex_key, &["client", "p2p", "collection", "add", &cols])
    }

    pub fn p2p_collection_list_with_identity(&self, hex_key: &str) -> Result<Value> {
        let out = self.exec_with_identity(hex_key, &["client", "p2p", "collection", "list"])?;
        serde_json::from_str(&out).wrap_err("failed to parse p2p_collection_list output")
    }

    pub fn p2p_collection_delete_with_identity(
        &self,
        collections: &[&str],
        hex_key: &str,
    ) -> Result<String> {
        let cols = collections.join(",");
        self.exec_with_identity(hex_key, &["client", "p2p", "collection", "delete", &cols])
    }

    pub fn p2p_document_add_with_identity(
        &self,
        doc_ids: &[&str],
        hex_key: &str,
    ) -> Result<String> {
        let mut args = vec!["client", "p2p", "document", "add"];
        args.extend(doc_ids);
        self.exec_with_identity(hex_key, &args)
    }

    pub fn p2p_document_list_with_identity(&self, hex_key: &str) -> Result<Value> {
        let out = self.exec_with_identity(hex_key, &["client", "p2p", "document", "list"])?;
        serde_json::from_str(&out).wrap_err("failed to parse p2p_document_list output")
    }

    pub fn p2p_document_delete_with_identity(
        &self,
        doc_ids: &[&str],
        hex_key: &str,
    ) -> Result<String> {
        let mut args = vec!["client", "p2p", "document", "delete"];
        args.extend(doc_ids);
        self.exec_with_identity(hex_key, &args)
    }

    pub fn p2p_replicator_set_with_identity(
        &self,
        collections: &[&str],
        addr: &str,
        hex_key: &str,
    ) -> Result<String> {
        let cols = collections.join(",");
        self.exec_with_identity(
            hex_key,
            &["client", "p2p", "replicator", "add", "-c", &cols, addr],
        )
    }

    pub fn p2p_replicator_list_with_identity(&self, hex_key: &str) -> Result<Value> {
        let out = self.exec_with_identity(hex_key, &["client", "p2p", "replicator", "list"])?;
        serde_json::from_str(&out).wrap_err("failed to parse p2p_replicator_list output")
    }

    pub fn p2p_replicator_delete_with_identity(
        &self,
        collections: &[&str],
        peer_addr: Option<&str>,
        hex_key: &str,
    ) -> Result<String> {
        let cols = collections.join(",");
        let mut args = vec!["client", "p2p", "replicator", "delete", "-c", &cols];
        if let Some(addr) = peer_addr {
            args.push(addr);
        }
        self.exec_with_identity(hex_key, &args)
    }
}
