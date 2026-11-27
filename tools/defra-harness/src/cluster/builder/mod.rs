use std::path::PathBuf;
use std::time::Duration;

use regolith::sync::OnceCell;
use reqwest::Client;

use crate::divergences::NodeKind;
use crate::error::{Result, WrapErr};
use crate::node::{
    start_node, BinarySource, GoNode, KeyringBackend, NodeConfig, RustNode, VeraNodeConfig,
};
use crate::ports::allocate_node_ports;

use super::health::health_check_all;
use super::runtime::TestCluster;

/// Attempts per node when startup fails on the guard-release → child-bind
/// TOCTOU window (another host process stealing the freed port). Each retry
/// re-allocates fresh ports, so consecutive collisions are vanishingly rare.
const PORT_CONFLICT_ATTEMPTS: usize = 3;

static RUST_BUILD_DONE: OnceCell<()> = OnceCell::new();
static IROH_BUILD_DONE: OnceCell<()> = OnceCell::new();

/// Reads the `DEFRA_MULTIPLIERS` env var and returns true if `signed-docs`
/// is present in its comma-separated value.
fn signed_docs_multiplier_active() -> bool {
    signed_docs_in(std::env::var("DEFRA_MULTIPLIERS").ok().as_deref())
}

/// Returns true if `value` is a comma-separated multiplier list that
/// contains `signed-docs` (case-sensitive, whitespace-trimmed per entry).
///
/// `DEFRA_MULTIPLIERS` is forward-compatible — unknown entries are
/// ignored, so e.g. `"signed-docs,foo"` matches.
///
/// Pulled out from `signed_docs_multiplier_active` so the parsing logic
/// can be unit-tested without mutating process env vars.
fn signed_docs_in(value: Option<&str>) -> bool {
    value
        .map(|v| v.split(',').any(|s| s.trim() == "signed-docs"))
        .unwrap_or(false)
}

pub struct TestClusterBuilder {
    rust_nodes: usize,
    go_nodes: usize,
    p2p_enabled: bool,
    health_timeout: Duration,
    rust_binary: Option<BinarySource>,
    go_binary: Option<BinarySource>,
    acp_document_type: Option<String>,
    node_identity: Option<String>,
    node_identities: Vec<Option<String>>,
    node_stores: Vec<Option<String>>,
    encryption_enabled: bool,
    signing_enabled: bool,
    nac_enabled: bool,
    vera: Option<VeraNodeConfig>,
    development: bool,
    store: Option<String>,
    query_timeout: Option<u64>,
    p2p_transport: Option<String>,
    keyring: KeyringBackend,
    shared_se_key: Option<[u8; 32]>,
    file_keyring: bool,
    acp_cache_ttl: Option<u64>,
    acp_circuit_breaker_threshold: Option<u32>,
    acp_circuit_breaker_reset_timeout: Option<u64>,
    acp_request_timeout: Option<u64>,
    acp_receipt_timeout: Option<u64>,
    signing_multiplier_opt_out: bool,
    extra_rust_args: Vec<String>,
    node_extra_rust_args: Vec<Vec<String>>,
    extra_go_args: Vec<String>,
}

impl Default for TestClusterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TestClusterBuilder {
    pub fn new() -> Self {
        Self {
            rust_nodes: 0,
            go_nodes: 0,
            p2p_enabled: false,
            health_timeout: Duration::from_secs(30),
            rust_binary: None,
            go_binary: None,
            acp_document_type: None,
            node_identity: None,
            node_identities: Vec::new(),
            node_stores: Vec::new(),
            encryption_enabled: false,
            signing_enabled: false,
            nac_enabled: false,
            vera: None,
            development: false,
            store: None,
            query_timeout: None,
            p2p_transport: None,
            keyring: KeyringBackend::None,
            shared_se_key: None,
            file_keyring: false,
            acp_cache_ttl: None,
            acp_circuit_breaker_threshold: None,
            acp_circuit_breaker_reset_timeout: None,
            acp_request_timeout: None,
            acp_receipt_timeout: None,
            signing_multiplier_opt_out: false,
            extra_rust_args: Vec::new(),
            node_extra_rust_args: Vec::new(),
            extra_go_args: Vec::new(),
        }
    }

    /// Append raw CLI flags to every Rust node's `start` invocation.
    ///
    /// The escape hatch for options with no typed builder method. Flags land
    /// after all managed ones, so they win under clap's last-one-wins parsing.
    ///
    /// Intended for enforcement tests -- set an option to a value that must
    /// change observable behavior, then assert the behavior changed. Prefer a
    /// typed `with_*` method for anything a test suite sets repeatedly.
    ///
    /// Rust-scoped on purpose: several options under test have no Go
    /// counterpart, and silently passing them to a Go node would abort startup.
    pub fn with_extra_rust_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extra_rust_args
            .extend(args.into_iter().map(Into::into));
        self
    }

    /// Append raw CLI flags to one Rust node's `start` invocation (by cluster
    /// index), after the cluster-wide [`with_extra_rust_args`](Self::with_extra_rust_args),
    /// so they win for that node. They survive a restart of the node.
    pub fn with_node_extra_rust_args<I, S>(mut self, index: usize, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        while self.node_extra_rust_args.len() <= index {
            self.node_extra_rust_args.push(Vec::new());
        }
        self.node_extra_rust_args[index].extend(args.into_iter().map(Into::into));
        self
    }

    /// The flags appended to Rust node `index`: the cluster-wide ones, then
    /// that node's own.
    fn rust_args_for(&self, index: usize) -> Vec<String> {
        self.extra_rust_args
            .iter()
            .chain(self.node_extra_rust_args.get(index).into_iter().flatten())
            .cloned()
            .collect()
    }

    /// Extra flags appended to every Go node's `start` command, after the
    /// managed flags (same contract as `with_extra_rust_args`).
    pub fn with_extra_go_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extra_go_args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn rust_nodes(mut self, n: usize) -> Self {
        self.rust_nodes = n;
        self
    }

    pub fn go_nodes(mut self, n: usize) -> Self {
        self.go_nodes = n;
        self
    }

    pub fn with_p2p(mut self) -> Self {
        self.p2p_enabled = true;
        self
    }

    pub fn health_timeout(mut self, d: Duration) -> Self {
        self.health_timeout = d;
        self
    }

    /// Set the binary source for Rust nodes.
    ///
    /// Defaults to `BinarySource::Workspace` (builds from workspace via cargo).
    pub fn with_rust_binary(mut self, source: BinarySource) -> Self {
        self.rust_binary = Some(source);
        self
    }

    /// Set the binary source for Go nodes.
    ///
    /// Defaults to `BinarySource::Path("defradb")` (found via PATH).
    pub fn with_go_binary(mut self, source: BinarySource) -> Self {
        self.go_binary = Some(source);
        self
    }

    /// Skip the cargo build step for Rust nodes.
    /// Equivalent to `.with_rust_binary(BinarySource::Path(workspace_binary_path()))`.
    pub fn skip_build(mut self) -> Self {
        self.rust_binary = Some(BinarySource::Path(RustNode::workspace_binary_path()));
        self
    }

    pub fn with_acp_local(mut self) -> Self {
        self.acp_document_type = Some("local".to_string());
        self
    }

    pub fn with_identity(mut self, key: impl Into<String>) -> Self {
        self.node_identity = Some(key.into());
        self
    }

    /// Set identity for a specific node (by index). Overrides cluster-wide identity.
    pub fn with_node_identity(mut self, index: usize, key: impl Into<String>) -> Self {
        while self.node_identities.len() <= index {
            self.node_identities.push(None);
        }
        self.node_identities[index] = Some(key.into());
        self
    }

    /// Override the store backend for a single node (by cluster index). Falls
    /// back to the cluster-wide `with_store`. Lets a mixed Go/Rust cluster give
    /// each node its implementation's native disk store — e.g. Rust `redb`, Go
    /// `badger` — so state survives a restart on either side.
    pub fn with_node_store(mut self, index: usize, store: impl Into<String>) -> Self {
        while self.node_stores.len() <= index {
            self.node_stores.push(None);
        }
        self.node_stores[index] = Some(store.into());
        self
    }

    pub fn with_encryption(mut self) -> Self {
        self.encryption_enabled = true;
        self
    }

    pub fn with_signing(mut self) -> Self {
        self.signing_enabled = true;
        self
    }

    /// Opt this cluster out of the `signed-docs` test multiplier.
    ///
    /// When `DEFRA_MULTIPLIERS` contains `signed-docs`, `build()` would
    /// normally force `signing_enabled = true`. Call this on tests that
    /// are known to be incompatible with signing.
    pub fn no_signing_multiplier(mut self) -> Self {
        self.signing_multiplier_opt_out = true;
        self
    }

    pub fn with_nac(mut self) -> Self {
        self.nac_enabled = true;
        self
    }

    /// Point every Rust node at a running vera.rs deployment through
    /// DefraDB's native Vera provider. The caller starts the deployment and
    /// keeps it alive for as long as the nodes need it. The provider journals
    /// its worker's key in the node's keyring, so nodes get a File keyring.
    pub fn with_vera(mut self, vera: VeraNodeConfig) -> Self {
        self.vera = Some(vera);
        self.file_keyring = true;
        self.acp_document_type = Some(crate::node::VERA_ACP_DOCUMENT_TYPE.to_string());
        self
    }

    pub fn with_development(mut self) -> Self {
        self.development = true;
        self
    }

    pub fn with_store(mut self, store: impl Into<String>) -> Self {
        self.store = Some(store.into());
        self
    }

    pub fn with_query_timeout(mut self, secs: u64) -> Self {
        self.query_timeout = Some(secs);
        self
    }

    pub fn with_iroh_transport(mut self) -> Self {
        self.p2p_transport = Some("iroh".to_string());
        self.p2p_enabled = true;
        self
    }

    pub fn with_acp_cache_ttl(mut self, secs: u64) -> Self {
        self.acp_cache_ttl = Some(secs);
        self
    }

    pub fn with_acp_circuit_breaker_threshold(mut self, threshold: u32) -> Self {
        self.acp_circuit_breaker_threshold = Some(threshold);
        self
    }

    pub fn with_acp_circuit_breaker_reset_timeout(mut self, secs: u64) -> Self {
        self.acp_circuit_breaker_reset_timeout = Some(secs);
        self
    }

    pub fn with_acp_request_timeout(mut self, secs: u64) -> Self {
        self.acp_request_timeout = Some(secs);
        self
    }

    pub fn with_acp_receipt_timeout(mut self, secs: u64) -> Self {
        self.acp_receipt_timeout = Some(secs);
        self
    }

    pub fn with_keyring(mut self) -> Self {
        self.keyring = KeyringBackend::Env {
            secret: "integration-test-secret".to_string(),
        };
        self
    }

    /// A per-node file keyring (`--keyring-backend file --keyring-path
    /// <rootdir>/keys`) on both runtimes, so peer identities survive
    /// restarts. With the `Env` keyring a Go node presents a new peer ID on
    /// every start, and any replicator pointed at it never reconnects.
    pub fn with_file_keyring(mut self) -> Self {
        self.file_keyring = true;
        self
    }

    /// Seed the same 32-byte searchable-encryption key into every node's
    /// keyring (Go and Rust) before start, mirroring how operators provision
    /// the cluster-shared SE secret per node. Forces a `File` keyring backend
    /// for all nodes so both runtimes read the same Go-compatible JWE entry,
    /// and each node's `getOrCreateSearchableEncryptionKey` finds the key
    /// instead of generating a fresh one.
    ///
    /// SE artifacts are HMAC search tags over field values keyed by this
    /// secret; cross-node SE queries only match when both nodes hold the same
    /// key. Implies a keyring (overrides `--no-keyring`).
    pub fn with_shared_searchable_encryption_key(mut self, key: [u8; 32]) -> Self {
        self.shared_se_key = Some(key);
        self
    }
}

mod build;
#[cfg(test)]
mod tests;
