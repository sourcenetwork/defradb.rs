//! Peer, replicator and document-sync commands.

use serde_json::Value;

use super::DefraClient;
use crate::error::{Result, WrapErr};

impl DefraClient {
    // ---- P2P operations ----

    /// Get P2P node info via `client p2p info`.
    pub fn p2p_info(&self) -> Result<Value> {
        let out = self.exec(&["client", "p2p", "info"])?;
        serde_json::from_str(&out).wrap_err("failed to parse p2p_info output")
    }

    /// Connect to peers via `client p2p connect <addr>...`.
    pub fn p2p_connect(&self, addrs: &[&str]) -> Result<String> {
        let mut args = vec!["client", "p2p", "connect"];
        args.extend(addrs);
        self.exec(&args)
    }

    /// Add P2P collections via `client p2p collection add <cols>`.
    pub fn p2p_collection_add(&self, collections: &[&str]) -> Result<String> {
        let cols = collections.join(",");
        self.exec(&["client", "p2p", "collection", "add", &cols])
    }

    /// Add a replicator via `client p2p replicator add -c <cols> <addr>`.
    pub fn p2p_replicator_set(&self, collections: &[&str], addr: &str) -> Result<String> {
        let cols = collections.join(",");
        self.exec(&["client", "p2p", "replicator", "add", "-c", &cols, addr])
    }

    /// List active peers via `client p2p active-peers`.
    pub fn p2p_active_peers(&self) -> Result<Value> {
        let out = self.exec(&["client", "p2p", "active-peers"])?;
        serde_json::from_str(&out).wrap_err("failed to parse p2p_active_peers output")
    }

    /// List P2P collections via `client p2p collection list`.
    pub fn p2p_collection_list(&self) -> Result<Value> {
        let out = self.exec(&["client", "p2p", "collection", "list"])?;
        serde_json::from_str(&out).wrap_err("failed to parse p2p_collection_list output")
    }

    /// Delete P2P collections via `client p2p collection delete <cols>`.
    pub fn p2p_collection_delete(&self, collections: &[&str]) -> Result<String> {
        let cols = collections.join(",");
        self.exec(&["client", "p2p", "collection", "delete", &cols])
    }

    /// List replicators via `client p2p replicator list`.
    pub fn p2p_replicator_list(&self) -> Result<Value> {
        let out = self.exec(&["client", "p2p", "replicator", "list"])?;
        serde_json::from_str(&out).wrap_err("failed to parse p2p_replicator_list output")
    }

    /// Delete a replicator via `client p2p replicator delete -c <cols> [peerID]`.
    pub fn p2p_replicator_delete(
        &self,
        collections: &[&str],
        peer_addr: Option<&str>,
    ) -> Result<String> {
        let cols = collections.join(",");
        let mut args = vec!["client", "p2p", "replicator", "delete", "-c", &cols];
        if let Some(addr) = peer_addr {
            args.push(addr);
        }
        self.exec(&args)
    }

    // ---- P2P Document operations ----

    /// Add documents to P2P subscription via `client p2p document add <id1> <id2>`.
    pub fn p2p_document_add(&self, doc_ids: &[&str]) -> Result<String> {
        let mut args = vec!["client", "p2p", "document", "add"];
        args.extend(doc_ids);
        self.exec(&args)
    }

    /// Remove documents from P2P subscription.
    pub fn p2p_document_delete(&self, doc_ids: &[&str]) -> Result<String> {
        let mut args = vec!["client", "p2p", "document", "delete"];
        args.extend(doc_ids);
        self.exec(&args)
    }

    /// List P2P document subscriptions.
    pub fn p2p_document_list(&self) -> Result<Value> {
        let out = self.exec(&["client", "p2p", "document", "list"])?;
        serde_json::from_str(&out).wrap_err("failed to parse p2p_document_list output")
    }

    /// Sync documents via `client p2p document sync <collection> <docIDs...>`.
    pub fn p2p_document_sync(&self, collection: &str, doc_ids: &[&str]) -> Result<String> {
        let mut args = vec!["client", "p2p", "document", "sync", collection];
        args.extend(doc_ids);
        self.exec(&args)
    }

    /// Sync collection versions via `client p2p collection sync-versions <versionIDs...>`.
    pub fn p2p_collection_sync_versions(&self, version_ids: &[&str]) -> Result<String> {
        let mut args = vec!["client", "p2p", "collection", "sync-versions"];
        args.extend(version_ids);
        self.exec(&args)
    }

    /// Sync branchable collection via `client p2p collection sync-branchable <id>`.
    pub fn p2p_collection_sync_branchable(&self, collection_id: &str) -> Result<String> {
        self.exec(&[
            "client",
            "p2p",
            "collection",
            "sync-branchable",
            collection_id,
        ])
    }
}
