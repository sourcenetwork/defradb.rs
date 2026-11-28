mod go_node;
mod running;
mod rust_node;

pub use go_node::GoNode;
pub use running::{start_node, RunningNode};
pub use rust_node::RustNode;

pub use crate::infra::ManagedProcess;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::divergences::NodeKind;

static NEXT_STAGING_ID: AtomicU64 = AtomicU64::new(0);

/// Where to obtain a DefraDB binary.
#[derive(Debug, Clone)]
pub enum BinarySource {
    /// Build from the local workspace via `cargo build -p cli`.
    /// This is the default for Rust nodes.
    Workspace,

    /// Build from the local workspace with extra cargo features (e.g. `["iroh"]`).
    WorkspaceWithFeatures(Vec<String>),

    /// Use a pre-existing binary at this absolute path. No build step.
    Path(PathBuf),

    /// Download a release binary by version tag (e.g. `"v0.20.0"`).
    /// The binary is cached in `~/.cache/defra-harness/<version>/`.
    Release(String),
}

impl BinarySource {
    /// Resolve the binary source to an absolute path, building or downloading as needed.
    pub fn resolve(&self, kind: NodeKind) -> crate::error::Result<PathBuf> {
        match self {
            BinarySource::Workspace => match kind {
                NodeKind::Rust => {
                    RustNode::build()?;
                    Ok(RustNode::workspace_binary_path())
                }
                NodeKind::Go => Ok(GoNode::path_binary()),
            },
            BinarySource::WorkspaceWithFeatures(features) => {
                let refs: Vec<&str> = features.iter().map(|s| s.as_str()).collect();
                RustNode::build_with_features(&refs)?;
                Ok(RustNode::workspace_binary_path_with_features(&refs))
            }
            BinarySource::Path(p) => {
                crate::error::ensure!(p.exists(), "binary not found at {}", p.display());
                Ok(p.clone())
            }
            BinarySource::Release(version) => resolve_release(version, kind),
        }
    }
}

/// Download (or return cached) a release binary for the given version and node kind.
fn resolve_release(version: &str, kind: NodeKind) -> crate::error::Result<PathBuf> {
    let cache_dir = release_cache_dir(&dirs_cache(), version, kind);
    let cached = cache_dir.join("defradb");
    if cached.exists() {
        return Ok(cached);
    }

    let asset_name = release_asset_name(version, kind)?;
    let repo = match kind {
        NodeKind::Rust => "sourcenetwork/defradb.rs",
        NodeKind::Go => "sourcenetwork/defradb",
    };
    let url = format!(
        "https://github.com/{}/releases/download/{}/{}",
        repo, version, asset_name
    );

    std::fs::create_dir_all(&cache_dir)?;
    let download = cache_dir.join(format!(".{asset_name}.download"));

    tracing::info!(%version, %url, "downloading release binary");

    let output = Command::new("curl")
        .args(["-fSL", "-o"])
        .arg(&download)
        .arg(&url)
        .output()
        .wrap_err_with(|| format!("failed to download {}", url))?;

    crate::error::ensure!(
        output.status.success(),
        "download failed ({}): {}",
        url,
        String::from_utf8_lossy(&output.stderr)
    );

    match kind {
        NodeKind::Rust => {
            let staging_id = NEXT_STAGING_ID.fetch_add(1, Ordering::Relaxed);
            let staging = StagingDir::new(
                cache_dir.join(format!(".extract-{}-{staging_id}", std::process::id())),
            )?;
            let output = Command::new("tar")
                .args(["-xzf"])
                .arg(&download)
                .arg("-C")
                .arg(&staging.path)
                .arg("defradb")
                .output()
                .wrap_err_with(|| format!("failed to extract {}", download.display()))?;
            if !output.status.success() {
                crate::error::bail!(
                    "failed to extract {}: {}",
                    download.display(),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            let extracted = staging.path.join("defradb");
            crate::error::ensure!(extracted.exists(), "release did not contain defradb");
            make_executable(&extracted)?;
            std::fs::rename(&extracted, &cached)?;
            std::fs::remove_file(&download)?;
        }
        NodeKind::Go => {
            make_executable(&download)?;
            std::fs::rename(&download, &cached)?;
        }
    }
    crate::error::ensure!(cached.exists(), "release did not contain defradb");

    Ok(cached)
}

fn make_executable(path: &Path) -> crate::error::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn release_cache_dir(cache: &Path, version: &str, kind: NodeKind) -> PathBuf {
    let kind = match kind {
        NodeKind::Rust => "rust",
        NodeKind::Go => "go",
    };
    cache.join("defra-harness").join(version).join(kind)
}

struct StagingDir {
    path: PathBuf,
}

impl StagingDir {
    fn new(path: PathBuf) -> crate::error::Result<Self> {
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn release_asset_name(version: &str, kind: NodeKind) -> crate::error::Result<String> {
    let (os, arch) = current_platform()?;
    let ver_num = version.strip_prefix('v').unwrap_or(version);
    let name = match kind {
        NodeKind::Go => format!("defradb_{}_{}", ver_num, os_arch_suffix(os, arch)),
        NodeKind::Rust => rust_release_asset_name(version, os, arch),
    };
    Ok(name)
}

fn rust_release_asset_name(version: &str, os: &str, arch: &str) -> String {
    format!(
        "defradb_{version}_{}.tar.gz",
        release_platform_suffix(os, arch)
    )
}

fn release_platform_suffix(os: &str, arch: &str) -> String {
    match (os, arch) {
        ("macos", "aarch64") => "darwin_arm64".to_string(),
        ("macos", "x86_64") => "darwin_amd64".to_string(),
        ("linux", "x86_64") => "linux_amd64".to_string(),
        ("linux", "aarch64") => "linux_arm64".to_string(),
        ("windows", "x86_64") => "windows_amd64".to_string(),
        _ => format!("{}_{}", os, arch),
    }
}

fn os_arch_suffix(os: &str, arch: &str) -> String {
    match (os, arch) {
        ("macos", "aarch64") => "darwin_arm64".to_string(),
        ("macos", "x86_64") => "darwin_x86_64".to_string(),
        ("linux", "x86_64") => "linux_x86_64".to_string(),
        ("linux", "aarch64") => "linux_arm64".to_string(),
        ("windows", "x86_64") => "windows_x86_64.exe".to_string(),
        _ => format!("{}_{}", os, arch),
    }
}

fn current_platform() -> crate::error::Result<(&'static str, &'static str)> {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        crate::error::bail!("unsupported OS for release download");
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        crate::error::bail!("unsupported architecture for release download");
    };

    Ok((os, arch))
}

fn dirs_cache() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(dir);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache");
    }
    PathBuf::from("/tmp")
}

use crate::error::WrapErr;

#[cfg(test)]
mod tests {
    use super::{release_cache_dir, rust_release_asset_name, StagingDir};
    use crate::divergences::NodeKind;

    #[test]
    fn rust_release_asset_matches_the_published_archive() {
        assert_eq!(
            rust_release_asset_name("v1.2.0", "linux", "x86_64"),
            "defradb_v1.2.0_linux_amd64.tar.gz"
        );
        assert_eq!(
            rust_release_asset_name("v1.2.0", "macos", "aarch64"),
            "defradb_v1.2.0_darwin_arm64.tar.gz"
        );
    }

    #[test]
    fn release_cache_is_namespaced_by_node_kind() {
        let cache = std::path::Path::new("/cache");
        let rust = release_cache_dir(cache, "v1.2.0", NodeKind::Rust);
        let go = release_cache_dir(cache, "v1.2.0", NodeKind::Go);
        assert_ne!(rust, go);
        assert!(rust.ends_with("v1.2.0/rust"));
        assert!(go.ends_with("v1.2.0/go"));
        assert_eq!(
            release_cache_dir(cache, "v1.2.0", NodeKind::Rust),
            release_cache_dir(cache, "v1.2.0", NodeKind::Rust)
        );
    }

    #[test]
    fn staging_directory_is_removed_on_drop() {
        let dir = crate::infra::TestRunDir::new(&std::env::temp_dir(), "DEFRA_STAGING_TEST_KEEP")
            .expect("create isolated staging directory");
        let path = dir.path().join("extract");
        let staging = StagingDir::new(path.clone()).expect("create staging directory");
        assert!(path.exists());
        drop(staging);
        assert!(!path.exists());
    }
}

/// How the DefraDB keyring is configured.
#[derive(Clone, Debug)]
pub enum KeyringBackend {
    /// No keyring (`--no-keyring` flag).
    None,
    /// Environment-based keyring (`DEFRA_KEYRING_SECRET` env var).
    Env { secret: String },
    /// File-based keyring (`--keyring-backend file --keyring-path <path>`).
    File { path: PathBuf, secret: String },
}

/// Configuration for DefraDB's Orbis signer integration.
///
/// When provided, DefraDB delegates document signing to an Orbis ring
/// via gRPC threshold signing instead of using a local key.
#[derive(Clone, Debug)]
pub struct OrbisSignerConfig {
    /// gRPC endpoint of an Orbis node (e.g. `"http://127.0.0.1:8081"`).
    pub endpoint: String,
    /// Id of the key derivation registered on Vera that the node signs with.
    pub derivation_id: String,
    /// The derived BLS public key (hex), provisioned independently of the ring.
    pub public_key: String,
    /// Hex Ed25519 key (seed followed by public key) the node authenticates
    /// to the ring with; the ring accepts only EdDSA tokens.
    pub identity: String,
}

/// The `--document-acp-type` selector of DefraDB's native Vera provider.
pub const VERA_ACP_DOCUMENT_TYPE: &str = "vera";

/// How a Rust node reaches its Vera deployment, as operators configure it.
#[derive(Clone, Debug)]
pub struct VeraNodeConfig {
    /// JSON-RPC endpoint of a Vera node.
    pub address: String,
    /// The deployment's consensus public key (hex).
    pub consensus_key: String,
    /// The deployment's genesis block identifier (hex).
    pub genesis_id: String,
    /// The deployment identifier that signed submissions bind to.
    pub deployment_id: u64,
}

/// Configuration for a single DefraDB node.
#[derive(Clone, Debug)]
pub struct NodeConfig {
    pub name: String,
    pub rootdir: PathBuf,
    pub log_dir: PathBuf,
    pub http_addr: String,
    pub p2p_enabled: bool,
    pub p2p_addr: Option<String>,
    pub peers: Vec<String>,
    pub identity: Option<String>,
    pub acp_document_type: Option<String>,
    pub encryption_enabled: bool,
    pub signing_enabled: bool,
    pub nac_enabled: bool,
    pub vera: Option<VeraNodeConfig>,
    pub orbis_signer: Option<OrbisSignerConfig>,
    pub keyring: KeyringBackend,
    pub development: bool,
    pub store: Option<String>,
    pub query_timeout: Option<u64>,
    pub p2p_transport: Option<String>,
    /// A cluster-shared searchable-encryption key (32-byte AES-256) to seed
    /// into this node's keyring before start. When `Some`, `start_node` runs
    /// `<binary> keyring add searchable-encryption-key <hex>` against the same
    /// keyring backend the node uses, so the node's `getOrCreate` finds it.
    /// Both Go and Rust read it -- the keyring JWE format is cross-compatible.
    pub shared_se_key: Option<[u8; 32]>,
    pub acp_cache_ttl: Option<u64>,
    pub acp_circuit_breaker_threshold: Option<u32>,
    pub acp_circuit_breaker_reset_timeout: Option<u64>,
    pub acp_request_timeout: Option<u64>,
    pub acp_receipt_timeout: Option<u64>,
    /// Raw CLI flags appended after every managed flag, so they win under
    /// clap's last-one-wins parsing. The escape hatch for options the builder
    /// has no typed method for -- notably enforcement tests, which must set an
    /// arbitrary option and prove it changes observable behavior.
    pub extra_args: Vec<String>,
}

impl NodeConfig {
    pub fn new(
        name: impl Into<String>,
        rootdir: PathBuf,
        log_dir: PathBuf,
        http_addr: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            rootdir,
            log_dir,
            http_addr: http_addr.into(),
            p2p_enabled: false,
            p2p_addr: None,
            peers: vec![],
            identity: None,
            acp_document_type: None,
            encryption_enabled: false,
            signing_enabled: false,
            nac_enabled: false,
            vera: None,
            orbis_signer: None,
            keyring: KeyringBackend::None,
            development: false,
            store: None,
            query_timeout: None,
            p2p_transport: None,
            shared_se_key: None,
            acp_cache_ttl: None,
            acp_circuit_breaker_threshold: None,
            acp_circuit_breaker_reset_timeout: None,
            acp_request_timeout: None,
            acp_receipt_timeout: None,
            extra_args: Vec::new(),
        }
    }
}

/// The keyring entry name DefraDB (Go + Rust) loads the cluster-shared
/// searchable-encryption key from. Mirrors Go's
/// `cli/start.go:getOrCreateSearchableEncryptionKey` ("searchable-encryption-key").
pub const SEARCHABLE_ENCRYPTION_KEY_NAME: &str = "searchable-encryption-key";

/// Seed a 32-byte searchable-encryption key into `config`'s keyring before the
/// node starts, by invoking `<binary> keyring add searchable-encryption-key
/// <hex>` against the same backend/path/secret the node will use on start.
///
/// The keyring JWE format is cross-compatible between Go and Rust, so one
/// shared key written here is read by both runtimes' `getOrCreate` at start.
///
/// Requires a `File` keyring backend (the only backend with a deterministic,
/// process-isolated on-disk location both runtimes can share in tests).
pub fn seed_searchable_encryption_key(
    binary: &Path,
    kind: NodeKind,
    config: &NodeConfig,
) -> crate::error::Result<()> {
    let key = match config.shared_se_key {
        Some(k) => k,
        None => return Ok(()),
    };
    let (path, secret) = match &config.keyring {
        KeyringBackend::File { path, secret } => (path.clone(), secret.clone()),
        other => {
            crate::error::bail!(
                "seeding a shared searchable-encryption key requires a File keyring backend, got {:?}",
                other
            );
        }
    };

    let hex_key: String = key.iter().map(|b| format!("{:02x}", b)).collect();

    // Global keyring flags come before the `keyring` subcommand for both CLIs.
    let mut args: Vec<String> = vec![
        "--rootdir".into(),
        config.rootdir.display().to_string(),
        "--keyring-backend".into(),
        "file".into(),
        "--keyring-path".into(),
        path.display().to_string(),
    ];
    // The Rust CLI gates `keyring add` behind --development; Go does not.
    if kind == NodeKind::Rust {
        args.push("--development".into());
    }
    args.extend([
        "keyring".into(),
        "add".into(),
        SEARCHABLE_ENCRYPTION_KEY_NAME.into(),
        hex_key,
    ]);

    std::fs::create_dir_all(&config.rootdir)?;
    let output = Command::new(binary)
        .args(&args)
        .env("DEFRA_KEYRING_SECRET", &secret)
        .output()
        .wrap_err("failed to run keyring add for searchable-encryption-key")?;

    crate::error::ensure!(
        output.status.success(),
        "keyring add searchable-encryption-key failed ({:?}): {}",
        kind,
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// Trait for building a DefraDB command from config.
pub trait DefraNode {
    fn kind(&self) -> NodeKind;
    /// Return (program, args, envs) for spawning via ManagedProcess.
    fn command_parts(&self, config: &NodeConfig) -> (PathBuf, Vec<String>, Vec<(String, String)>);
    fn api_url(host: &str, port: u16) -> String
    where
        Self: Sized,
    {
        format!("http://{}:{}", host, port)
    }
    fn prepare(&self) -> crate::error::Result<()> {
        Ok(())
    }
    fn binary_path(&self) -> &Path;
}
