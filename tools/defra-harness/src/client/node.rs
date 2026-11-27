//! Node, block, backup and dump commands.

use serde_json::Value;

use super::DefraClient;
use crate::error::{Result, WrapErr};

impl DefraClient {
    // ---- Node/Block operations ----

    /// Get node identity.
    pub fn node_identity(&self) -> Result<Value> {
        let out = self.exec(&["client", "node-identity"])?;
        let trimmed = out.trim();
        serde_json::from_str(trimmed).or_else(|_| Ok(Value::String(trimmed.to_string())))
    }

    /// Verify a block signature.
    pub fn block_verify_signature(
        &self,
        public_key: &str,
        cid: &str,
        key_type: Option<&str>,
    ) -> Result<String> {
        let mut args = vec!["client", "block", "verify-signature", public_key, cid];
        if let Some(kt) = key_type {
            args.push("--key-type");
            args.push(kt);
        }
        self.exec(&args)
    }

    // ---- Backup operations ----

    /// Export backup.
    pub fn backup_export(&self, file: &str, collections: &[&str], pretty: bool) -> Result<String> {
        let mut args = vec!["client", "backup", "export", file];
        for c in collections {
            args.push("-c");
            args.push(c);
        }
        if pretty {
            args.push("--pretty");
        }
        self.exec(&args)
    }

    /// Import backup.
    pub fn backup_import(&self, file: &str) -> Result<String> {
        self.exec(&["client", "backup", "import", file])
    }

    // ---- Dump operations ----

    /// Dump database contents.
    pub fn dump(&self) -> Result<Value> {
        let out = self.exec(&["client", "dump"])?;
        serde_json::from_str(&out).wrap_err("failed to parse dump output")
    }
}
