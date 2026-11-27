//! Collection management commands.

use std::process::Command;

use serde_json::Value;

use super::DefraClient;
use crate::divergences::NodeKind;
use crate::error::{Result, WrapErr};

impl DefraClient {
    // ---- Collection management ----

    /// Patch a collection schema.
    pub fn collection_patch(&self, patch: &str) -> Result<String> {
        self.exec(&["client", "collection", "patch", patch])
    }

    /// Set active collection version.
    pub fn collection_set_active(&self, version_id: &str) -> Result<String> {
        self.exec(&["client", "collection", "set-active", version_id])
    }

    /// Purge the database.
    pub fn purge(&self) -> Result<String> {
        self.exec(&["client", "purge", "--force"])
    }

    /// Get collection version info including VersionID.
    ///
    /// A Rust node answers with the active version as one object, read from
    /// Go's `GET /api/v0/collections?name=<name>`; a Go node answers with the
    /// CLI's array.
    pub fn collection_describe_version(&self, name: &str) -> Result<Value> {
        match self.kind {
            NodeKind::Rust => {
                let url = format!("{}/api/v0/collections?name={}", self.url, name);
                let output = Command::new("curl")
                    .arg("-s")
                    .arg("-f")
                    .arg(&url)
                    .output()
                    .wrap_err_with(|| format!("failed to curl {}", url))?;
                if !output.status.success() {
                    crate::error::bail!(
                        "GET {} failed ({}): {}",
                        url,
                        output.status,
                        String::from_utf8_lossy(&output.stderr).trim()
                    );
                }
                let versions: Vec<Value> = serde_json::from_slice(&output.stdout)
                    .wrap_err("failed to parse collections output")?;
                versions
                    .into_iter()
                    .next()
                    .wrap_err_with(|| format!("collection '{}' not found", name))
            }
            NodeKind::Go => {
                let out = self.exec(&[
                    "client",
                    "collection",
                    "describe",
                    "--collection-name",
                    name,
                ])?;
                serde_json::from_str(&out).wrap_err("failed to parse collection describe output")
            }
        }
    }

    /// The VersionID of every version named `name`, active or not, read from
    /// `GET /api/v0/collections?name=<name>&get_inactive=true`.
    pub fn collection_version_ids(&self, name: &str) -> Result<Vec<String>> {
        let url = format!(
            "{}/api/v0/collections?name={}&get_inactive=true",
            self.url, name
        );
        let output = Command::new("curl")
            .arg("-s")
            .arg("-f")
            .arg(&url)
            .output()
            .wrap_err_with(|| format!("failed to curl {}", url))?;
        if !output.status.success() {
            crate::error::bail!(
                "GET {} failed ({}): {}",
                url,
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let versions: Vec<Value> = serde_json::from_slice(&output.stdout)
            .wrap_err("failed to parse collections output")?;
        versions
            .iter()
            .map(|version| {
                version["VersionID"]
                    .as_str()
                    .map(str::to_string)
                    .wrap_err_with(|| format!("no VersionID in {}", version))
            })
            .collect()
    }

    /// Get collection version info using an identity (for NAC-enabled nodes).
    pub fn collection_describe_version_with_identity(
        &self,
        name: &str,
        hex_key: &str,
    ) -> Result<Value> {
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
        let val: Value =
            serde_json::from_str(&out).wrap_err("failed to parse collection describe output")?;
        if val.get("CollectionID").is_none() && val.get("VersionID").is_none() {
            if let Some(id) = val.get("ID").and_then(|v| v.as_str()) {
                let mut out_val = val.clone();
                out_val["CollectionID"] = serde_json::Value::String(id.to_string());
                if let Some(ver) = val
                    .get("SchemaVersionID")
                    .and_then(|v| v.as_str())
                    .or_else(|| val.get("VersionID").and_then(|v| v.as_str()))
                {
                    out_val["VersionID"] = serde_json::Value::String(ver.to_string());
                }
                return Ok(out_val);
            }
        }
        Ok(val)
    }
}
