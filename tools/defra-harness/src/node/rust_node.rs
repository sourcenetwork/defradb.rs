use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Result, WrapErr};

use super::{DefraNode, KeyringBackend, NodeConfig};
use crate::divergences::{self, NodeKind};
use crate::workspace_root;

type Parts = (PathBuf, Vec<String>, Vec<(String, String)>);

/// A Rust DefraDB node backed by the `defra` binary.
pub struct RustNode {
    binary_path: PathBuf,
}

impl RustNode {
    /// Point to the debug binary in the workspace target dir.
    pub fn from_workspace() -> Self {
        Self {
            binary_path: Self::workspace_binary_path(),
        }
    }

    /// Use a pre-existing binary at the given path.
    pub fn from_binary(path: impl Into<PathBuf>) -> Self {
        Self {
            binary_path: path.into(),
        }
    }

    /// The default workspace binary path (`target/debug/defradb`).
    pub fn workspace_binary_path() -> PathBuf {
        workspace_root().join("target/debug/defradb")
    }

    /// The workspace binary built with `features`, kept apart from the default
    /// build so one feature set cannot replace another's executable.
    pub fn workspace_binary_path_with_features(features: &[&str]) -> PathBuf {
        Self::feature_target_dir(features).join("debug/defradb")
    }

    /// Build the Rust binary via cargo (debug mode for fast iteration).
    pub fn build() -> Result<()> {
        let status = Command::new("cargo")
            .args(["build", "-p", "cli"])
            .current_dir(workspace_root())
            .status()
            .wrap_err("failed to run cargo build")?;

        crate::error::ensure!(status.success(), "cargo build failed with {}", status);
        Ok(())
    }

    /// Build the Rust binary with additional cargo features enabled.
    pub fn build_with_features(features: &[&str]) -> Result<()> {
        let features_str = features.join(",");
        let status = Command::new("cargo")
            .args([
                "build",
                "-p",
                "cli",
                "--features",
                &features_str,
                "--target-dir",
            ])
            .arg(Self::feature_target_dir(features))
            .current_dir(workspace_root())
            .status()
            .wrap_err("failed to run cargo build with features")?;

        crate::error::ensure!(status.success(), "cargo build failed with {}", status);
        Ok(())
    }

    fn feature_target_dir(features: &[&str]) -> PathBuf {
        let mut features: Vec<&str> = features
            .iter()
            .copied()
            .filter(|feature| !feature.is_empty())
            .collect();
        features.sort_unstable();
        features.dedup();
        let key = if features.is_empty() {
            "default".to_string()
        } else {
            features.join("+")
        };
        workspace_root().join("target/defra-harness").join(key)
    }

    /// Verify the Rust binary is built and returns parseable version info.
    pub fn check_available() -> Result<()> {
        let binary = workspace_root().join("target/debug/defradb");

        let output = Command::new(&binary)
            .args(["version", "--format", "json"])
            .output()
            .wrap_err("defra binary not found — run `cargo build -p cli` first")?;

        crate::error::ensure!(
            output.status.success(),
            "defra version failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let _json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .wrap_err("failed to parse defra version JSON")?;

        Ok(())
    }
}

impl DefraNode for RustNode {
    fn kind(&self) -> NodeKind {
        NodeKind::Rust
    }

    fn command_parts(&self, config: &NodeConfig) -> Parts {
        let mut args = vec![
            "--rootdir".to_string(),
            config.rootdir.display().to_string(),
            "--url".to_string(),
            config.http_addr.clone(),
            "--no-log-color".to_string(),
            "--log-output".to_string(),
            "stdout".to_string(),
        ];

        let mut envs: Vec<(String, String)> = Vec::new();

        match &config.keyring {
            KeyringBackend::None => args.push("--no-keyring".to_string()),
            KeyringBackend::Env { secret } => {
                envs.push(("DEFRA_KEYRING_SECRET".to_string(), secret.clone()));
            }
            KeyringBackend::File { path, secret } => {
                args.extend([
                    "--keyring-backend".into(),
                    "file".into(),
                    "--keyring-path".into(),
                    path.display().to_string(),
                ]);
                envs.push(("DEFRA_KEYRING_SECRET".to_string(), secret.clone()));
            }
        }

        args.push("start".to_string());
        args.push("--store".to_string());
        args.push(config.store.as_deref().unwrap_or("memory").to_string());
        args.push("--no-telemetry".to_string());

        if !config.encryption_enabled {
            args.push("--no-encryption".to_string());
            args.push("--no-searchable-encryption".to_string());
        }
        if let Some(ref signer) = config.orbis_signer {
            args.extend([
                "--signer-type".into(),
                "orbis".into(),
                "--signer-orbis-endpoint".into(),
                signer.endpoint.clone(),
                "--signer-orbis-derivation-id".into(),
                signer.derivation_id.clone(),
                "--signer-orbis-public-key".into(),
                signer.public_key.clone(),
                "--signer-orbis-identity".into(),
                signer.identity.clone(),
            ]);
            // An Orbis ring signs with BLS, which Go peers cannot verify;
            // the clusters that sign through a ring are Rust-only.
            envs.push((
                "DEFRA_ALLOW_NON_GO_VERIFIABLE_SIGNING".to_string(),
                "1".to_string(),
            ));
        } else if !config.signing_enabled {
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

        // DIVERGENCE: Only Rust supports --vera-* flags
        if divergences::supports_vera_flags(NodeKind::Rust) {
            if let Some(ref vera) = config.vera {
                args.extend([
                    "--vera-address".into(),
                    vera.address.clone(),
                    "--vera-consensus-key".into(),
                    vera.consensus_key.clone(),
                    "--vera-genesis-id".into(),
                    vera.genesis_id.clone(),
                    "--vera-deployment-id".into(),
                    vera.deployment_id.to_string(),
                ]);
            }
        }

        if let Some(ref transport) = config.p2p_transport {
            args.push("--p2p-transport".to_string());
            args.push(transport.clone());
        }

        if config.development {
            args.push("--development".to_string());
        }

        if let Some(timeout) = config.query_timeout {
            args.push("--query-timeout".to_string());
            args.push(timeout.to_string());
        }

        if let Some(ttl) = config.acp_cache_ttl {
            args.push("--acp-cache-ttl".to_string());
            args.push(ttl.to_string());
        }
        if let Some(threshold) = config.acp_circuit_breaker_threshold {
            args.push("--acp-circuit-breaker-threshold".to_string());
            args.push(threshold.to_string());
        }
        if let Some(timeout) = config.acp_circuit_breaker_reset_timeout {
            args.push("--acp-circuit-breaker-reset-timeout".to_string());
            args.push(timeout.to_string());
        }
        if let Some(timeout) = config.acp_request_timeout {
            args.push("--acp-request-timeout".to_string());
            args.push(timeout.to_string());
        }
        if let Some(timeout) = config.acp_receipt_timeout {
            args.push("--acp-receipt-timeout".to_string());
            args.push(timeout.to_string());
        }

        args.extend(config.extra_args.iter().cloned());

        (self.binary_path.clone(), args, envs)
    }

    fn binary_path(&self) -> &Path {
        &self.binary_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> NodeConfig {
        NodeConfig::new(
            "n0",
            PathBuf::from("/tmp/defra-test-root"),
            PathBuf::from("/tmp/defra-test-log"),
            "127.0.0.1:9181",
        )
    }

    #[test]
    fn extra_args_are_appended_after_managed_flags() {
        let node = RustNode::from_binary("/nonexistent/defra");
        let mut config = test_config();
        config.extra_args = vec!["--max-schema-size".to_string(), "2048".to_string()];

        let (_binary, args, _envs) = node.command_parts(&config);

        assert_eq!(
            &args[args.len() - 2..],
            &["--max-schema-size".to_string(), "2048".to_string()],
            "extra args must land last so they override managed flags"
        );
    }

    #[test]
    fn extra_args_default_to_none() {
        let node = RustNode::from_binary("/nonexistent/defra");
        let (_binary, args, _envs) = node.command_parts(&test_config());

        assert!(!args.iter().any(|a| a == "--max-schema-size"));
    }

    #[test]
    fn feature_builds_have_a_target_separate_from_the_default_build() {
        assert_ne!(
            RustNode::workspace_binary_path(),
            RustNode::workspace_binary_path_with_features(&["iroh"])
        );
        assert_eq!(
            RustNode::workspace_binary_path_with_features(&["iroh", "orbis"]),
            RustNode::workspace_binary_path_with_features(&["orbis", "iroh"])
        );
    }

    #[test]
    fn vera_passes_the_deployment_trust_material() {
        let node = RustNode::from_binary("/nonexistent/defra");
        let mut config = test_config();
        config.vera = Some(super::super::VeraNodeConfig {
            address: "http://127.0.0.1:8545".to_string(),
            consensus_key: "aabb".to_string(),
            genesis_id: "cc".repeat(32),
            deployment_id: 9003,
        });

        let (_binary, args, _envs) = node.command_parts(&config);

        let value = |flag: &str| {
            let position = args
                .iter()
                .position(|argument| argument == flag)
                .unwrap_or_else(|| panic!("{flag} should be present"));
            args[position + 1].clone()
        };
        assert_eq!(value("--vera-address"), "http://127.0.0.1:8545");
        assert_eq!(value("--vera-consensus-key"), "aabb");
        assert_eq!(value("--vera-genesis-id"), "cc".repeat(32));
        assert_eq!(value("--vera-deployment-id"), "9003");
    }
}
