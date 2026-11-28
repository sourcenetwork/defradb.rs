//! Secondary and encrypted index commands.

use serde_json::Value;

use super::DefraClient;
use crate::divergences::{self};
use crate::error::{Result, WrapErr};

impl DefraClient {
    // ---- Index operations ----

    /// Create an index.
    ///
    /// DIVERGENCE:
    /// - Rust: `client index create <collection> --fields <f>` (positional)
    /// - Go:   `client index create --collection <c> --fields <f>` (flag)
    pub fn index_create(
        &self,
        collection: &str,
        fields: &[&str],
        name: Option<&str>,
        unique: bool,
    ) -> Result<Value> {
        let fields_csv = fields.join(",");
        let use_flag = !divergences::index_uses_positional_args(self.kind);
        let out = self.build_index_create_args(collection, &fields_csv, name, unique, use_flag)?;
        serde_json::from_str(&out).wrap_err("failed to parse index_create output")
    }

    pub(super) fn build_index_create_args(
        &self,
        collection: &str,
        fields_csv: &str,
        name: Option<&str>,
        unique: bool,
        use_flag: bool,
    ) -> Result<String> {
        let mut args = vec!["client", "index", "new"];
        if use_flag {
            args.push("--collection");
        }
        args.push(collection);
        args.push("--fields");
        args.push(fields_csv);
        if let Some(n) = name {
            args.push("--name");
            args.push(n);
        }
        if unique {
            args.push("--unique");
        }
        self.exec(&args)
    }

    /// List indexes.
    ///
    /// DIVERGENCE: Rust positional, Go `--collection` flag.
    pub fn index_list(&self, collection: Option<&str>) -> Result<Value> {
        let out = if let Some(c) = collection {
            if divergences::index_uses_positional_args(self.kind) {
                self.exec(&["client", "index", "list", c])?
            } else {
                self.exec(&["client", "index", "list", "--collection", c])?
            }
        } else {
            self.exec(&["client", "index", "list"])?
        };
        serde_json::from_str(&out).wrap_err("failed to parse index_list output")
    }

    /// Delete an index.
    ///
    /// DIVERGENCE: Rust positional, Go `--collection`/`--name` flags.
    pub fn index_delete(&self, collection: &str, name: &str) -> Result<String> {
        if divergences::index_uses_positional_args(self.kind) {
            self.exec(&["client", "index", "delete", collection, name])
        } else {
            self.exec(&[
                "client",
                "index",
                "delete",
                "--collection",
                collection,
                "--name",
                name,
            ])
        }
    }

    // ---- Encrypted Index operations ----

    /// Add an encrypted index.
    ///
    /// DIVERGENCE:
    /// - Rust: `client encrypted-index new <collection> <field>` (positional)
    /// - Go:   `client encrypted-index new --collection <c> --field <f>` (flags)
    pub fn encrypted_index_add(&self, collection: &str, field: &str) -> Result<Value> {
        let out = if divergences::encrypted_index_uses_positional_args(self.kind) {
            self.exec(&["client", "encrypted-index", "new", collection, field])?
        } else {
            self.exec(&[
                "client",
                "encrypted-index",
                "new",
                "--collection",
                collection,
                "--field",
                field,
            ])?
        };
        serde_json::from_str(&out).wrap_err("failed to parse encrypted_index_add output")
    }

    /// Delete an encrypted index.
    ///
    /// DIVERGENCE: Rust positional, Go flags.
    pub fn encrypted_index_delete(&self, collection: &str, field: &str) -> Result<String> {
        if divergences::encrypted_index_uses_positional_args(self.kind) {
            self.exec(&["client", "encrypted-index", "delete", collection, field])
        } else {
            self.exec(&[
                "client",
                "encrypted-index",
                "delete",
                "--collection",
                collection,
                "--field",
                field,
            ])
        }
    }

    /// List encrypted indexes.
    ///
    /// DIVERGENCE: Rust positional, Go flags.
    pub fn encrypted_index_list(&self, collection: &str) -> Result<Value> {
        let out = if divergences::encrypted_index_uses_positional_args(self.kind) {
            self.exec(&["client", "encrypted-index", "list", collection])?
        } else {
            self.exec(&[
                "client",
                "encrypted-index",
                "list",
                "--collection",
                collection,
            ])?
        };
        serde_json::from_str(&out).wrap_err("failed to parse encrypted_index_list output")
    }
}
