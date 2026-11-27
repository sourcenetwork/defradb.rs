mod acp;
mod as_identity;
mod as_identity_collection;
mod as_identity_p2p;
mod collection;
mod document;
mod index;
mod node;
mod p2p;
mod query;
mod schema;
mod txn;

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Result, WrapErr};
use serde_json::Value;

use crate::divergences::NodeKind;

/// CLI-based client for DefraDB.
///
/// Executes commands against a running node using the `client` subcommand tree.
/// `NodeKind` discriminates between Go and Rust implementations so every
/// behavioral difference is an explicit match arm rather than a silent fallback.
pub struct DefraClient {
    binary_path: PathBuf,
    url: String,
    kind: NodeKind,
}

impl DefraClient {
    pub fn new(binary_path: impl Into<PathBuf>, url: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            binary_path: binary_path.into(),
            url: url.into(),
            kind,
        }
    }

    pub fn binary_path(&self) -> &Path {
        &self.binary_path
    }

    pub fn kind(&self) -> NodeKind {
        self.kind
    }

    pub(super) fn exec(&self, args: &[&str]) -> Result<String> {
        let root = crate::infra::TestRunDir::new(
            &std::env::temp_dir().join("defra-client"),
            "DEFRA_E2E_KEEP",
        )?;
        let output = Command::new(&self.binary_path)
            .arg("--rootdir")
            .arg(root.path())
            .arg("--url")
            .arg(&self.url)
            .args(args)
            .output()
            .wrap_err_with(|| {
                format!(
                    "failed to exec: {} --url {} {}",
                    self.binary_path.display(),
                    self.url,
                    args.join(" ")
                )
            })?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            Err(crate::error::err!(
                "command failed (exit {}): stderr={}, stdout={}",
                output.status,
                stderr.trim(),
                stdout.trim()
            ))
        }
    }

    pub(super) fn exec_with_identity(&self, hex_key: &str, args: &[&str]) -> Result<String> {
        let mut full_args = vec!["client", "-i", hex_key];
        let skip = if args.first() == Some(&"client") {
            1
        } else {
            0
        };
        full_args.extend(&args[skip..]);
        self.exec(&full_args)
    }

    // ---- Query output normalization ----

    /// Parse query output, handling the Go/Rust format divergence.
    ///
    /// Both Go and Rust wrap output in `{"data": ...}`.
    /// Go also prefixes with a "Request Results" header line.
    pub(super) fn parse_query_output(&self, out: &str) -> Result<Value> {
        // Go CLI prefixes output with "------ Request Results ------\n"
        let json_str = out.find('{').map(|i| &out[i..]).unwrap_or(out);
        let val: Value = serde_json::from_str(json_str).wrap_err("failed to parse query output")?;
        if let Some(data) = val.get("data") {
            Ok(data.clone())
        } else {
            Ok(val)
        }
    }
}
