use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Result, WrapErr};

use super::{DefraNode, KeyringBackend, NodeConfig};
use crate::divergences::{self, NodeKind};

/// A Go DefraDB node backed by the `defradb` binary.
pub struct GoNode {
    binary_path: PathBuf,
}

impl GoNode {
    /// Create a GoNode using the `defradb` binary from PATH.
    pub fn from_path() -> Self {
        Self {
            binary_path: Self::path_binary(),
        }
    }

    /// Use a pre-existing binary at the given path.
    pub fn from_binary(path: impl Into<PathBuf>) -> Self {
        Self {
            binary_path: path.into(),
        }
    }

    /// The default PATH-based binary name.
    pub fn path_binary() -> PathBuf {
        PathBuf::from("defradb")
    }

    /// Verify the Go binary is available and version-compatible.
    ///
    /// Compares the Go binary's commit against `DEFRA_GO_COMPAT_COMMIT` env var.
    /// Set `DEFRA_SKIP_VERSION_CHECK=1` to bypass the check.
    pub fn check_available() -> Result<()> {
        let output = Command::new("defradb")
            .args(["version", "--format", "json"])
            .output()
            .wrap_err("defradb binary not found in PATH")?;

        crate::error::ensure!(
            output.status.success(),
            "defradb version failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .wrap_err("failed to parse defradb version JSON")?;

        let go_commit = json["commit"].as_str().unwrap_or("unknown");

        let expected = std::env::var("DEFRA_GO_COMPAT_COMMIT").unwrap_or_default();
        if !expected.is_empty() && !go_commit.starts_with(&expected) {
            if std::env::var("DEFRA_SKIP_VERSION_CHECK").as_deref() == Ok("1") {
                tracing::warn!(
                    expected = %expected,
                    actual = go_commit,
                    "Go binary version mismatch (skipped via DEFRA_SKIP_VERSION_CHECK)"
                );
            } else {
                crate::error::bail!(
                    "Go binary version mismatch: expected commit starting with {expected}, got {go_commit}. \
                     Set DEFRA_SKIP_VERSION_CHECK=1 to bypass."
                );
            }
        }

        Ok(())
    }
}

impl DefraNode for GoNode {
    fn kind(&self) -> NodeKind {
        NodeKind::Go
    }

    fn command_parts(&self, config: &NodeConfig) -> (PathBuf, Vec<String>, Vec<(String, String)>) {
        let mut args = vec![
            "--rootdir".to_string(),
            config.rootdir.display().to_string(),
            "--no-log-color".to_string(),
            "--log-output".to_string(),
            "stdout".to_string(),
        ];

        // DefraDB v1.0.0: `--url` and the keyring flags are `start` subcommand
        // flags, not root persistent flags, so they must be passed AFTER `start`.
        let mut envs: Vec<(String, String)> = Vec::new();
        let mut start_flags: Vec<String> = vec!["--url".to_string(), config.http_addr.clone()];
        match &config.keyring {
            KeyringBackend::None => start_flags.push("--no-keyring".to_string()),
            KeyringBackend::Env { secret } => {
                envs.push(("DEFRA_KEYRING_SECRET".to_string(), secret.clone()));
            }
            KeyringBackend::File { path, secret } => {
                start_flags.extend([
                    "--keyring-backend".into(),
                    "file".into(),
                    "--keyring-path".into(),
                    path.display().to_string(),
                ]);
                envs.push(("DEFRA_KEYRING_SECRET".to_string(), secret.clone()));
            }
        }

        args.push("start".to_string());
        args.extend(start_flags);
        args.extend([
            "--store".to_string(),
            config.store.as_deref().unwrap_or("memory").to_string(),
            "--no-telemetry".to_string(),
        ]);

        if !config.encryption_enabled {
            args.push("--no-encryption".to_string());
            args.push("--no-searchable-encryption".to_string());
        }
        if !config.signing_enabled {
            args.push("--no-signing".to_string());
        }

        if config.p2p_enabled {
            if let Some(ref addr) = config.p2p_addr {
                args.push("--p2paddr".to_string());
                args.push(addr.clone());
            }
            for peer in &config.peers {
                args.push("--peers".to_string());
                args.push(peer.clone());
            }
        } else {
            args.push("--no-p2p".to_string());
        }

        if let Some(ref identity) = config.identity {
            args.push("--identity".to_string());
            args.push(identity.clone());
        }

        if let Some(ref acp_type) = config.acp_document_type {
            args.push("--document-acp-type".to_string());
            args.push(acp_type.clone());
        }

        if config.nac_enabled {
            args.push("--node-acp-enable".to_string());
        }

        // DIVERGENCE: Go does not support --vera-* flags
        if config.vera.is_some() && divergences::supports_vera_flags(NodeKind::Go) {
            unreachable!("Go node does not support Vera flags");
        }

        if config.development {
            args.push("--development".to_string());
        }

        if let Some(timeout) = config.query_timeout {
            args.push("--query-timeout".to_string());
            args.push(timeout.to_string());
        }

        args.extend(config.extra_args.iter().cloned());

        (self.binary_path.clone(), args, envs)
    }

    fn binary_path(&self) -> &Path {
        &self.binary_path
    }
}
