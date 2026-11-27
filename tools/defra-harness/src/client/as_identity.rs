//! Commands issued under an identity.

use serde_json::Value;

use super::DefraClient;
use crate::divergences::{self};
use crate::error::{Result, WrapErr};

impl DefraClient {
    // ---- Identity-aware variants (NAC testing) ----

    pub fn p2p_info_with_identity(&self, hex_key: &str) -> Result<Value> {
        let out = self.exec_with_identity(hex_key, &["client", "p2p", "info"])?;
        serde_json::from_str(&out).wrap_err("failed to parse p2p_info output")
    }

    pub fn p2p_active_peers_with_identity(&self, hex_key: &str) -> Result<Value> {
        let out = self.exec_with_identity(hex_key, &["client", "p2p", "active-peers"])?;
        serde_json::from_str(&out).wrap_err("failed to parse p2p_active_peers output")
    }

    pub fn encrypted_index_add_with_identity(
        &self,
        collection: &str,
        field: &str,
        hex_key: &str,
    ) -> Result<Value> {
        let out = if divergences::encrypted_index_uses_positional_args(self.kind) {
            self.exec_with_identity(
                hex_key,
                &["client", "encrypted-index", "new", collection, field],
            )?
        } else {
            self.exec_with_identity(
                hex_key,
                &[
                    "client",
                    "encrypted-index",
                    "new",
                    "--collection",
                    collection,
                    "--field",
                    field,
                ],
            )?
        };
        serde_json::from_str(&out).wrap_err("failed to parse encrypted_index_add output")
    }

    pub fn encrypted_index_list_with_identity(
        &self,
        collection: &str,
        hex_key: &str,
    ) -> Result<Value> {
        let out = if divergences::encrypted_index_uses_positional_args(self.kind) {
            self.exec_with_identity(hex_key, &["client", "encrypted-index", "list", collection])?
        } else {
            self.exec_with_identity(
                hex_key,
                &[
                    "client",
                    "encrypted-index",
                    "list",
                    "--collection",
                    collection,
                ],
            )?
        };
        serde_json::from_str(&out).wrap_err("failed to parse encrypted_index_list output")
    }

    pub fn encrypted_index_delete_with_identity(
        &self,
        collection: &str,
        field: &str,
        hex_key: &str,
    ) -> Result<String> {
        if divergences::encrypted_index_uses_positional_args(self.kind) {
            self.exec_with_identity(
                hex_key,
                &["client", "encrypted-index", "delete", collection, field],
            )
        } else {
            self.exec_with_identity(
                hex_key,
                &[
                    "client",
                    "encrypted-index",
                    "delete",
                    "--collection",
                    collection,
                    "--field",
                    field,
                ],
            )
        }
    }

    pub fn lens_add_with_identity(&self, config: &str, hex_key: &str) -> Result<Value> {
        let out = self.exec_with_identity(hex_key, &["client", "lens", "add", config])?;
        serde_json::from_str(&out).wrap_err("failed to parse lens_add output")
    }

    pub fn lens_list_with_identity(&self, hex_key: &str) -> Result<Value> {
        let out = self.exec_with_identity(hex_key, &["client", "lens", "list"])?;
        serde_json::from_str(&out).wrap_err("failed to parse lens_list output")
    }

    pub fn lens_set_with_identity(
        &self,
        src: &str,
        dst: &str,
        config: &str,
        hex_key: &str,
    ) -> Result<Value> {
        let out = self.exec_with_identity(hex_key, &["client", "lens", "set", src, dst, config])?;
        serde_json::from_str(&out).wrap_err("failed to parse lens_set output")
    }

    pub fn view_add_with_identity(
        &self,
        gql_query: &str,
        sdl: &str,
        hex_key: &str,
    ) -> Result<Value> {
        let out = self.exec_with_identity(
            hex_key,
            &["client", "view", "add", "--query", gql_query, "--sdl", sdl],
        )?;
        serde_json::from_str(&out).wrap_err("failed to parse view_add output")
    }

    pub fn view_refresh_with_identity(&self, name: Option<&str>, hex_key: &str) -> Result<Value> {
        let out = if let Some(n) = name {
            self.exec_with_identity(
                hex_key,
                &["client", "view", "refresh", "--collection-name", n],
            )?
        } else {
            self.exec_with_identity(hex_key, &["client", "view", "refresh"])?
        };
        let trimmed = out.trim();
        if trimmed.is_empty() {
            return Ok(serde_json::json!({}));
        }
        serde_json::from_str(trimmed).wrap_err("failed to parse view_refresh output")
    }

    pub fn p2p_document_sync_with_identity(
        &self,
        collection: &str,
        doc_ids: &[&str],
        hex_key: &str,
    ) -> Result<String> {
        let mut args = vec!["client", "p2p", "document", "sync", collection];
        args.extend(doc_ids);
        self.exec_with_identity(hex_key, &args)
    }

    pub fn p2p_collection_sync_versions_with_identity(
        &self,
        version_ids: &[&str],
        hex_key: &str,
    ) -> Result<String> {
        let mut args = vec!["client", "p2p", "collection", "sync-versions"];
        args.extend(version_ids);
        self.exec_with_identity(hex_key, &args)
    }

    pub fn p2p_collection_sync_branchable_with_identity(
        &self,
        collection_id: &str,
        hex_key: &str,
    ) -> Result<String> {
        self.exec_with_identity(
            hex_key,
            &[
                "client",
                "p2p",
                "collection",
                "sync-branchable",
                collection_id,
            ],
        )
    }
}
