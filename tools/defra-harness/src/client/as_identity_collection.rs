//! Collection and index commands issued under an identity.

use serde_json::Value;

use super::DefraClient;
use crate::divergences::{self};
use crate::error::{Result, WrapErr};

impl DefraClient {
    // ---- Identity-aware collection operations ----

    pub fn collection_list_with_identity(&self, hex_key: &str) -> Result<Vec<String>> {
        let out = self.exec_with_identity(hex_key, &["client", "collection", "list"])?;
        Self::parse_collection_list(&out)
    }

    pub fn collection_describe_with_identity(&self, name: &str, hex_key: &str) -> Result<Value> {
        let out = self.exec_with_identity(
            hex_key,
            &[
                "client",
                "collection",
                "describe",
                "--collection-name",
                name,
            ],
        )?;
        serde_json::from_str(&out).wrap_err("failed to parse collection_describe output")
    }

    pub fn collection_create_with_identity(
        &self,
        name: &str,
        doc: &str,
        hex_key: &str,
    ) -> Result<Value> {
        let out = self.exec_with_identity(
            hex_key,
            &["client", "document", "add", "--collection-name", name, doc],
        )?;
        let trimmed = out.trim();
        if trimmed.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(trimmed).wrap_err("failed to parse collection_create output")
    }

    pub fn collection_delete_with_identity(
        &self,
        name: &str,
        doc_id: &str,
        hex_key: &str,
    ) -> Result<String> {
        self.exec_with_identity(
            hex_key,
            &[
                "client",
                "document",
                "delete",
                "--collection-name",
                name,
                "--docID",
                doc_id,
            ],
        )
    }

    pub fn collection_update_with_identity(
        &self,
        name: &str,
        doc_id: &str,
        updater: &str,
        hex_key: &str,
    ) -> Result<String> {
        self.exec_with_identity(
            hex_key,
            &[
                "client",
                "document",
                "update",
                "--collection-name",
                name,
                "--docID",
                doc_id,
                "--updater",
                updater,
            ],
        )
    }

    pub fn collection_truncate_with_identity(&self, name: &str, hex_key: &str) -> Result<String> {
        self.exec_with_identity(
            hex_key,
            &[
                "client",
                "collection",
                "truncate",
                "--collection-name",
                name,
            ],
        )
    }

    pub fn collection_patch_with_identity(&self, patch: &str, hex_key: &str) -> Result<String> {
        self.exec_with_identity(hex_key, &["client", "collection", "patch", patch])
    }

    // ---- Identity-aware index operations ----

    pub fn index_create_with_identity(
        &self,
        collection: &str,
        fields: &[&str],
        name: Option<&str>,
        unique: bool,
        hex_key: &str,
    ) -> Result<Value> {
        let fields_csv = fields.join(",");
        let use_flag = !divergences::index_uses_positional_args(self.kind);
        let mut args = vec!["client", "index", "new"];
        if use_flag {
            args.push("--collection");
        }
        args.push(collection);
        args.push("--fields");
        args.push(&fields_csv);
        if let Some(n) = name {
            args.push("--name");
            args.push(n);
        }
        if unique {
            args.push("--unique");
        }
        let out = self.exec_with_identity(hex_key, &args)?;
        serde_json::from_str(&out).wrap_err("failed to parse index_create output")
    }

    pub fn index_list_with_identity(
        &self,
        collection: Option<&str>,
        hex_key: &str,
    ) -> Result<Value> {
        let out = if let Some(c) = collection {
            if divergences::index_uses_positional_args(self.kind) {
                self.exec_with_identity(hex_key, &["client", "index", "list", c])?
            } else {
                self.exec_with_identity(hex_key, &["client", "index", "list", "--collection", c])?
            }
        } else {
            self.exec_with_identity(hex_key, &["client", "index", "list"])?
        };
        serde_json::from_str(&out).wrap_err("failed to parse index_list output")
    }

    pub fn index_delete_with_identity(
        &self,
        collection: &str,
        name: &str,
        hex_key: &str,
    ) -> Result<String> {
        if divergences::index_uses_positional_args(self.kind) {
            self.exec_with_identity(hex_key, &["client", "index", "delete", collection, name])
        } else {
            self.exec_with_identity(
                hex_key,
                &[
                    "client",
                    "index",
                    "delete",
                    "--collection",
                    collection,
                    "--name",
                    name,
                ],
            )
        }
    }
}
