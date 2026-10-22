#[cfg(feature = "iroh")]
mod access_hooks;
mod error;
mod node;
mod node_acp;
mod node_identity;
mod node_p2p;
#[cfg(any(feature = "libp2p", feature = "iroh"))]
mod node_peer_key;
mod node_recovery;
mod node_tasks;
mod peer_keyring;

use std::sync::Arc;

use async_trait::async_trait;
pub use defra_p2p_adapter::{ReplicatorPushOptions, ReplicatorPushOptionsState};
pub use error::{Error, Result};

#[cfg(feature = "iroh")]
pub use access_hooks::AccessHooks;
pub use keyring;
pub use keyring::Keyring;
#[cfg(feature = "iroh")]
pub use node::build_with_store_and_access_hooks;
pub use node::{
    build_with_store, build_with_store_on_pool, start_node_pool, EmbeddedNode, NodeBuilder,
};
pub use node_tasks::BackgroundTasks;
pub use peer_keyring::PeerKeyring;
pub use zeroize::Zeroizing;

type ReplicatorPushOptionsCallback =
    Arc<dyn Fn(ReplicatorPushOptions) -> Result<(), String> + Send + Sync>;

/// On-demand replicator retry trigger. Runs a single retry pass that re-pushes
/// failed doc blocks AND regenerates/re-pushes their SE artifacts. Backs the
/// `p2p_retry_replicators` FFI op, mirroring Go's `RetryReplicators`.
type RetryReplicatorsCallback =
    Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync>;

/// Storage persistence hints for ACP/NAC setup.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Persistence {
    #[default]
    Memory,
    Persistent,
}

/// Supported runtime transports for embedded nodes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportConfig {
    #[default]
    None,
    #[cfg(feature = "libp2p")]
    Libp2p(Libp2pConfig),
    #[cfg(feature = "iroh")]
    Iroh(IrohConfig),
}

/// Libp2p transport configuration.
#[cfg(feature = "libp2p")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Libp2pConfig {
    pub listen_addr: String,
}

/// Iroh transport configuration.
///
/// # Migrating
///
/// This struct gained an `allowlist` field. It is not `#[non_exhaustive]`,
/// so an existing struct literal that names every field no longer compiles:
/// either add `allowlist: Default::default()`, which keeps exactly the
/// behaviour the code had (accept every inbound peer), or switch the
/// literal to `..Default::default()` so a later field costs nothing. Only
/// construction is affected; a node built from a literal that compiled
/// before behaves identically after.
#[cfg(feature = "iroh")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IrohConfig {
    pub bind_addr: Option<std::net::IpAddr>,
    pub bind_port: Option<u16>,
    pub relay_mode: p2p::iroh::IrohRelayModeConfig,
    pub discovery: p2p::iroh::IrohDiscoveryConfig,
    pub max_concurrent_multipath_paths: Option<u32>,
    /// Iroh key file from before the node shared one peer key across transports.
    /// Imported as the node's peer key when the store has none; never written.
    pub secret_key_path: Option<std::path::PathBuf>,
    /// Who may open an inbound connection. `AcceptAll` (the default) keeps
    /// the behaviour every existing embedder has. `Explicit` restricts
    /// inbound connections to the listed endpoint ids, and
    /// `P2POperations::allow_peer` can add more while the node runs. That
    /// runtime call is a no-op under `AcceptAll`, so an embedder that never
    /// sets this cannot turn the allowlist on at all.
    pub allowlist: p2p::iroh::IrohAllowlistConfig,
}

/// Node signing configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum SigningConfig {
    #[default]
    Disabled,
    Enabled {
        key: Option<SigningKey>,
    },
    RegisteredIdentity {
        did: String,
    },
}

/// Explicit node signing key material.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SigningKey {
    Secp256k1(Vec<u8>),
    Secp256r1(Vec<u8>),
    Ed25519(Vec<u8>),
}

/// Document ACP configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum DocumentAcpConfig {
    #[default]
    Local,
    #[cfg(feature = "vera")]
    Vera(VeraConfig),
    /// Vera ACP with distinct LCD and gRPC endpoints.
    #[cfg(feature = "vera")]
    VeraWithLcd {
        config: VeraConfig,
        lcd_address: String,
    },
}

/// Vera document ACP configuration.
#[cfg(feature = "vera")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VeraConfig {
    pub grpc_address: String,
    pub comet_rpc_address: String,
    pub chain_id: String,
    pub signer_key: Vec<u8>,
}

/// Node assembly configuration used by `build_with_store`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EmbeddedNodeConfig {
    /// Optional application-owned keyring for the transport identity.
    pub peer_keyring: Option<PeerKeyring>,
    pub persistence: Persistence,
    pub transport: TransportConfig,
    /// The network runtime the node's networking runs on: its transport,
    /// its database's network work and the pool's stuck-poll monitor. A node
    /// with a transport that names none runs them on the process's own
    /// ([`edge::on_demand`]); a node without networking starts none.
    pub edge: Option<edge::Edge>,
    pub signing: SigningConfig,
    pub document_acp: DocumentAcpConfig,
    pub query_limits: query::QueryLimits,
    pub max_concurrent_dag_fetches: Option<usize>,
    pub max_concurrent_push_tasks: Option<usize>,
    pub max_doc_sync_request_doc_ids: Option<usize>,
    pub rate_limit_burst: Option<u32>,
    pub rate_limit_rate: Option<f64>,
}

/// Runtime P2P transport kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportKind {
    #[cfg(feature = "libp2p")]
    Libp2p,
    #[cfg(feature = "iroh")]
    Iroh,
}

/// P2P runtime managed by an embedded node.
pub struct ManagedP2PSystem {
    kind: TransportKind,
    ops: Arc<dyn defra_http::P2POperations>,
    replicator_push_options: ReplicatorPushOptionsState,
    on_replicator_push_options: Option<ReplicatorPushOptionsCallback>,
    retry_replicators: regolith::sync::OnceCell<RetryReplicatorsCallback>,
    /// Outbound management requester (Task 7a): relays management requests to
    /// P2P-only peers on behalf of an HTTP caller. Set once during P2P setup
    /// (after the system is built), consumed when wiring `AppState`.
    manage_requester: regolith::sync::OnceCell<Arc<dyn defra_http::ManageRequester>>,
    shutdown: node::ShutdownHandle,
}

impl ManagedP2PSystem {
    pub fn new(
        kind: TransportKind,
        ops: Arc<dyn defra_http::P2POperations>,
        shutdown: node::ShutdownHandle,
    ) -> Self {
        Self::with_replicator_push_options(
            kind,
            ops,
            shutdown,
            ReplicatorPushOptionsState::default(),
        )
    }

    pub fn with_replicator_push_options(
        kind: TransportKind,
        ops: Arc<dyn defra_http::P2POperations>,
        shutdown: node::ShutdownHandle,
        replicator_push_options: ReplicatorPushOptionsState,
    ) -> Self {
        Self::with_replicator_push_options_callback(
            kind,
            ops,
            shutdown,
            replicator_push_options,
            None,
        )
    }

    pub fn with_replicator_push_options_callback(
        kind: TransportKind,
        ops: Arc<dyn defra_http::P2POperations>,
        shutdown: node::ShutdownHandle,
        replicator_push_options: ReplicatorPushOptionsState,
        on_replicator_push_options: Option<ReplicatorPushOptionsCallback>,
    ) -> Self {
        Self {
            kind,
            ops,
            replicator_push_options,
            on_replicator_push_options,
            retry_replicators: regolith::sync::OnceCell::new(),
            manage_requester: regolith::sync::OnceCell::new(),
            shutdown,
        }
    }

    /// Install the outbound management requester. Set once during P2P setup.
    pub fn set_manage_requester(&self, requester: Arc<dyn defra_http::ManageRequester>) {
        let _ = self.manage_requester.set(requester);
    }

    /// Get the outbound management requester, if installed.
    pub fn manage_requester(&self) -> Option<&Arc<dyn defra_http::ManageRequester>> {
        self.manage_requester.get()
    }

    /// Install the on-demand replicator retry trigger. Set once during P2P setup.
    pub fn set_retry_replicators(&self, callback: RetryReplicatorsCallback) {
        let _ = self.retry_replicators.set(callback);
    }

    /// Run a single on-demand replicator retry pass (re-push failed doc blocks
    /// and regenerate/re-push their SE artifacts). Backs `p2p_retry_replicators`.
    pub async fn retry_replicators(&self) -> Result<(), String> {
        match self.retry_replicators.get() {
            Some(callback) => {
                callback().await;
                Ok(())
            }
            None => Err("retry_replicators trigger not installed".to_string()),
        }
    }

    pub fn kind(&self) -> TransportKind {
        self.kind
    }

    pub fn ops(&self) -> &Arc<dyn defra_http::P2POperations> {
        &self.ops
    }

    /// The P2P operations serving the request `context` serves: their node
    /// access checks check the identity acting in it.
    pub fn ops_for(
        &self,
        context: &defra_core::RequestContext,
    ) -> Arc<dyn defra_http::P2POperations> {
        self.ops
            .bind_request(context)
            .unwrap_or_else(|| Arc::clone(&self.ops))
    }

    pub fn set_replicator_push_options(
        &self,
        options: ReplicatorPushOptions,
    ) -> Result<(), String> {
        self.replicator_push_options.store(options.clone())?;
        if let Some(callback) = &self.on_replicator_push_options {
            callback(options)?;
        }
        Ok(())
    }

    pub fn replicator_push_options(&self) -> ReplicatorPushOptions {
        self.replicator_push_options.load()
    }

    pub async fn shutdown(&self) {
        self.shutdown.shutdown().await;
    }
}

/// How the public `NodeBuilder` stores data.
///
/// regolith is the store, whether it is keeping the database in memory or
/// on disk, so the choice here is only whether anything is persisted and
/// whether values are encrypted at rest.
#[non_exhaustive]
pub enum EmbeddedStore {
    /// A regolith database, in memory or on a path.
    Regolith(storage::RegolithStore),
}

impl storage::corekv::private::Sealed for EmbeddedStore {}

#[async_trait]
impl storage::Store for EmbeddedStore {
    fn crdts(&self) -> &storage::Crdts {
        match self {
            Self::Regolith(store) => storage::Store::crdts(store),
        }
    }

    async fn allocate(&self, key: &[u8], count: u64) -> storage::Result<std::ops::Range<u64>> {
        match self {
            Self::Regolith(store) => store.allocate(key, count).await,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn transaction_stats_handle(&self) -> Option<storage::backends::TransactionStatsHandle> {
        match self {
            Self::Regolith(store) => store.transaction_stats_handle(),
        }
    }

    async fn new_txn(&self, readonly: bool) -> storage::Result<Box<dyn storage::Txn>> {
        match self {
            Self::Regolith(store) => store.new_txn(readonly).await,
        }
    }

    async fn transact_retry(
        &self,
        options: storage::corekv::RetryOptions,
        attempt: storage::corekv::RetryAttempt,
    ) -> std::result::Result<storage::corekv::RetryAttemptValue, storage::corekv::RetryAttemptError>
    {
        match self {
            Self::Regolith(store) => store.transact_retry(options, attempt).await,
        }
    }

    async fn close(&self) -> storage::Result<()> {
        match self {
            Self::Regolith(store) => store.close().await,
        }
    }
}
