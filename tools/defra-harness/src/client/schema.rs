//! Schema, lens and view commands.

use serde_json::Value;

use super::DefraClient;
use crate::error::{Result, WrapErr};

impl DefraClient {
    // ---- Schema operations ----

    /// List schema type names via GraphQL introspection.
    pub fn schema_describe(&self) -> Result<String> {
        let result = self.query(r#"{ __schema { types { name } } }"#)?;
        Ok(result.to_string())
    }

    // ---- Lens operations ----

    /// Add a lens migration.
    pub fn lens_add(&self, config: &str) -> Result<Value> {
        let out = self.exec(&["client", "lens", "add", config])?;
        serde_json::from_str(&out).wrap_err("failed to parse lens_add output")
    }

    /// List lens migrations.
    pub fn lens_list(&self) -> Result<Value> {
        let out = self.exec(&["client", "lens", "list"])?;
        serde_json::from_str(&out).wrap_err("failed to parse lens_list output")
    }

    /// Set a lens migration between schema versions.
    pub fn lens_set(&self, src: &str, dst: &str, config: &str) -> Result<Value> {
        let out = self.exec(&["client", "lens", "set", src, dst, config])?;
        serde_json::from_str(&out).wrap_err("failed to parse lens_set output")
    }

    /// Reload lens migrations.
    pub fn lens_reload(&self) -> Result<String> {
        self.exec(&["client", "lens", "reload"])
    }

    // ---- View operations ----

    /// Add a view.
    pub fn view_add(&self, gql_query: &str, sdl: &str) -> Result<Value> {
        let out = self.exec(&["client", "view", "add", "--query", gql_query, "--sdl", sdl])?;
        serde_json::from_str(&out).wrap_err("failed to parse view_add output")
    }

    /// Add a view with a lens transform CID.
    pub fn view_add_with_lens(&self, gql_query: &str, sdl: &str, lens_cid: &str) -> Result<Value> {
        let out = self.exec(&[
            "client",
            "view",
            "add",
            "--query",
            gql_query,
            "--sdl",
            sdl,
            "--lens-cid",
            lens_cid,
        ])?;
        serde_json::from_str(&out).wrap_err("failed to parse view_add output")
    }

    /// Refresh views.
    pub fn view_refresh(&self, name: Option<&str>) -> Result<Value> {
        let out = if let Some(n) = name {
            self.exec(&["client", "view", "refresh", "--collection-name", n])?
        } else {
            self.exec(&["client", "view", "refresh"])?
        };
        let trimmed = out.trim();
        if trimmed.is_empty() {
            return Ok(serde_json::json!({}));
        }
        serde_json::from_str(trimmed).wrap_err("failed to parse view_refresh output")
    }
}
