//! Configuration types for the embedded DefraDB node.

use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "http")]
use db::{DEFAULT_TRANSACTION_CLEANUP_INTERVAL, DEFAULT_TRANSACTION_IDLE_TIMEOUT};

/// Document ACP configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum DocumentAcpConfig {
    #[default]
    Local,
    /// On-chain document ACP via Vera.
    ///
    /// Requires the `vera` feature (on by default).
    #[cfg(feature = "vera")]
    Vera(VeraConfig),
    /// On-chain document ACP via Vera with distinct LCD and gRPC
    /// endpoints.
    #[cfg(feature = "vera")]
    VeraWithLcd {
        config: VeraConfig,
        lcd_address: String,
    },
}

/// Vera document ACP configuration.
///
/// Requires the `vera` feature (on by default).
#[cfg(feature = "vera")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VeraConfig {
    pub grpc_address: String,
    pub comet_rpc_address: String,
    pub chain_id: String,
    pub signer_key: Vec<u8>,
}

/// Configuration for the optional HTTP GraphQL server.
#[cfg(feature = "http")]
pub struct HttpConfig {
    pub address: std::net::SocketAddr,
    pub(crate) request_timeout: Duration,
    pub(crate) transaction_idle_timeout: Duration,
    pub(crate) transaction_cleanup_interval: Duration,
    pub(crate) extra_routes: Option<axum::Router>,
    pub(crate) router_transform: Option<defra_http::RouterTransform>,
    pub(crate) listener: Option<std::net::TcpListener>,
    pub(crate) dev_mode: bool,
}

#[cfg(feature = "http")]
impl HttpConfig {
    pub fn new(port: u16) -> Self {
        Self::with_addr(std::net::SocketAddr::from(([127, 0, 0, 1], port)))
    }

    pub fn with_addr(addr: impl Into<std::net::SocketAddr>) -> Self {
        Self {
            address: addr.into(),
            request_timeout: Duration::from_secs(
                defra_http::ServerConfig::default().request_timeout,
            ),
            transaction_idle_timeout: DEFAULT_TRANSACTION_IDLE_TIMEOUT,
            transaction_cleanup_interval: DEFAULT_TRANSACTION_CLEANUP_INTERVAL,
            extra_routes: None,
            router_transform: None,
            listener: None,
            dev_mode: false,
        }
    }

    /// Serve on a listener the caller has already bound.
    ///
    /// Binding first is the only way to serve on an ephemeral port (bind
    /// port 0) and know which one it is: choosing a free port, releasing it
    /// and handing over the address leaves a window in which another process
    /// can take it. The node serves on this listener even if `address` is
    /// changed afterwards.
    pub fn with_listener(listener: std::net::TcpListener) -> std::io::Result<Self> {
        let mut config = Self::with_addr(listener.local_addr()?);
        config.listener = Some(listener);
        Ok(config)
    }

    /// Set the HTTP request timeout.
    ///
    /// `Duration::ZERO` disables request timeouts.
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Set the max idle age for explicit HTTP transactions.
    ///
    /// `Duration::ZERO` disables idle transaction cleanup.
    pub fn with_transaction_idle_timeout(mut self, timeout: Duration) -> Self {
        self.transaction_idle_timeout = timeout;
        self
    }

    /// Set the interval between explicit HTTP transaction cleanup sweeps.
    ///
    /// Must be non-zero when transaction idle cleanup is enabled.
    pub fn with_transaction_cleanup_interval(mut self, interval: Duration) -> Self {
        self.transaction_cleanup_interval = interval;
        self
    }

    pub fn with_extra_routes(mut self, extra: axum::Router) -> Self {
        self.extra_routes = Some(extra);
        self
    }

    /// Wrap the complete HTTP router after native and application routes are
    /// assembled, so application middleware can cover both and their fallback.
    pub fn with_router_transform(mut self, transform: defra_http::RouterTransform) -> Self {
        self.router_transform = Some(transform);
        self
    }

    /// Let HTTP callers name lens modules by a path on the server's disk, as
    /// the CLI's `--development` does.
    pub fn with_dev_mode(mut self, dev_mode: bool) -> Self {
        self.dev_mode = dev_mode;
        self
    }
}

/// Storage persistence hints for ACP/NAC setup.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Persistence {
    #[default]
    Memory,
    Persistent,
}

/// Where a node's signing identity comes from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum NodeSigning {
    #[default]
    Disabled,
    /// The node creates its identity, from `key` or freshly generated, and
    /// registers it in the signing registry.
    Enabled { key: Option<SigningKey> },
    /// An identity already in the signing registry, by DID. It needs a local
    /// key or a remote signer.
    RegisteredIdentity { did: String },
}

/// Explicit node signing key material.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SigningKey {
    Secp256k1(Vec<u8>),
    Secp256r1(Vec<u8>),
    Ed25519(Vec<u8>),
}

/// The P2P transport a node runs.
// A config is built once per node, so the size of its largest variant costs nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub enum TransportConfig {
    #[default]
    None,
    #[cfg(feature = "libp2p")]
    Libp2p(Libp2pConfig),
    #[cfg(feature = "p2p")]
    Iroh(P2PConfig),
}

/// libp2p transport configuration.
#[cfg(feature = "libp2p")]
#[derive(Debug, Clone, PartialEq)]
pub struct Libp2pConfig {
    /// The addresses the host listens on; none leaves it dialing out only.
    pub listen_addrs: Vec<String>,
    /// Peers the host connects to once it listens.
    pub bootstrap_peers: Vec<String>,
    /// Run GossipSub, which document topics replicate over.
    pub enable_pubsub: bool,
    /// Run the relay client transport.
    pub enable_relay: bool,
    /// Seconds a block-sync stream may take; `None` keeps the host default.
    pub stream_timeout_secs: Option<u64>,
    /// Sync limits and the replicator retry ladder.
    pub sync: SyncTuning,
    /// Speak Go `shinzo-develop`'s wire (its CAR channel and batched
    /// pushes), so the node replicates with Go Shinzo nodes. Off keeps
    /// today's protocol.
    pub shinzo_wire: bool,
    /// libp2p resource budget overrides (Go Shinzo's `net.resourceMemoryMiB`,
    /// `net.resourceFileDescriptors`, `net.maxStreamsPerPeer`).
    pub resource_limits: p2p::ResourceLimits,
}

#[cfg(feature = "libp2p")]
impl Default for Libp2pConfig {
    fn default() -> Self {
        Self {
            listen_addrs: Vec::new(),
            bootstrap_peers: Vec::new(),
            enable_pubsub: true,
            enable_relay: true,
            stream_timeout_secs: None,
            sync: SyncTuning::default(),
            shinzo_wire: false,
            resource_limits: p2p::ResourceLimits::default(),
        }
    }
}

/// Sync limits and the replicator retry ladder, for either transport.
#[cfg(any(feature = "p2p", feature = "libp2p"))]
#[derive(Debug, Clone, PartialEq)]
pub struct SyncTuning {
    /// Maximum concurrent DAG fetch tasks. Lower values reduce resource pressure
    /// on constrained clients (mobile, embedded). Default: 4.
    pub max_concurrent_dag_fetches: usize,
    /// Maximum concurrent push tasks for sending blocks to replicators.
    /// Default: 8.
    pub max_concurrent_push_tasks: usize,
    /// Maximum document IDs accepted in a single DocSync request. Default: 1000.
    pub max_doc_sync_request_doc_ids: usize,
    /// Per-peer rate limit burst capacity (max tokens in bucket). Default: 500.
    pub rate_limit_burst: u32,
    /// Per-peer rate limit refill rate (tokens per second). Default: 50.
    pub rate_limit_rate: f64,
    /// Maximum pending-DAG registrations held while Bitswap completes missing
    /// links; overflow is nacked back to the pusher. Each source peer may use
    /// at most one quarter of this capacity. Default: 1000.
    pub max_pending_dags: usize,
    /// Seconds between replicator retries, the last one repeating; `None`
    /// keeps the Go-compatible default ladder.
    pub replicator_retry_intervals: Option<Vec<u64>>,
}

#[cfg(any(feature = "p2p", feature = "libp2p"))]
impl Default for SyncTuning {
    fn default() -> Self {
        let defaults = p2p::sync::SyncConfig::default();
        Self {
            max_concurrent_dag_fetches: defaults.max_concurrent_dag_fetches,
            max_concurrent_push_tasks: defaults.max_concurrent_push_tasks,
            max_doc_sync_request_doc_ids: defaults.max_doc_sync_request_doc_ids,
            rate_limit_burst: defaults.rate_limit_burst,
            rate_limit_rate: defaults.rate_limit_rate,
            max_pending_dags: defaults.max_pending_dags,
            replicator_retry_intervals: None,
        }
    }
}

#[cfg(any(feature = "p2p", feature = "libp2p"))]
impl SyncTuning {
    pub(crate) fn sync_config(&self) -> p2p::sync::SyncConfig {
        p2p::sync::SyncConfig {
            max_concurrent_dag_fetches: self.max_concurrent_dag_fetches,
            max_concurrent_push_tasks: self.max_concurrent_push_tasks,
            max_doc_sync_request_doc_ids: self.max_doc_sync_request_doc_ids,
            rate_limit_burst: self.rate_limit_burst,
            rate_limit_rate: self.rate_limit_rate,
            max_pending_dags: self.max_pending_dags,
            ..Default::default()
        }
    }

    pub(crate) fn retry_schedule(&self) -> crate::Result<storage::stores::RetrySchedule> {
        match &self.replicator_retry_intervals {
            Some(intervals) => storage::stores::RetrySchedule::new(intervals.clone())
                .map_err(crate::Error::InvalidRetrySchedule),
            None => Ok(storage::stores::RetrySchedule::default()),
        }
    }
}

/// Where an iroh node keeps its secret key.
#[cfg(feature = "p2p")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum PeerKeySource {
    /// In the file at [`P2PConfig::secret_key_path`], created there when
    /// absent. With no path the key is new on every start.
    #[default]
    File,
    /// In the node's store, one key shared with the libp2p transport, or in
    /// the application keyring ([`NodeConfig::peer_keyring`]). Here
    /// `secret_key_path` only names an iroh key file from before the node
    /// shared one peer key: it is imported when the store has none and never
    /// written.
    Store,
}

/// Configuration for the optional P2P networking layer.
///
/// Iroh/QUIC. Requires the `p2p` feature, which implies `native`.
#[cfg(feature = "p2p")]
#[derive(Debug, Clone, PartialEq)]
pub struct P2PConfig {
    /// UDP port for QUIC listener. `None` leaves the port to iroh.
    pub port: Option<u16>,
    /// Bind to a specific IP address. When set, IROH only listens on this
    /// interface -- use the Tailscale IP to keep P2P within the mesh and
    /// prevent IROH from advertising unreachable LAN addresses across sites.
    /// None = 0.0.0.0 (all interfaces).
    pub bind_addr: Option<std::net::IpAddr>,
    /// Relay behavior for NAT traversal.
    pub relay_mode: p2p::iroh::IrohRelayModeConfig,
    /// Address publishing / lookup behavior.
    pub discovery: p2p::iroh::IrohDiscoveryConfig,
    /// Inbound-connection authorization. Defaults to accepting every peer;
    /// restrict to an explicit set of iroh endpoint ids once this node is
    /// reachable from the internet (relay-enabled).
    pub allowlist: p2p::iroh::IrohAllowlistConfig,
    /// Maximum concurrent QUIC paths per connection. None keeps iroh's default.
    pub max_concurrent_multipath_paths: Option<u32>,
    /// Path of the secret key file; what it means depends on `peer_key`.
    pub secret_key_path: Option<std::path::PathBuf>,
    /// Where the secret key lives.
    pub peer_key: PeerKeySource,
    /// Reload collection subscriptions persisted in the local store on startup.
    /// When false, only explicit subscribe calls in the current process take effect.
    pub load_persisted_collections: bool,
    /// Sync limits and the replicator retry ladder.
    pub sync: SyncTuning,
    /// Re-announce blocks merged from peers on their gossip topics, so
    /// subscribers with no transport route to the author still receive them
    /// through whichever peer merged them. Signatures are verified on merge,
    /// so a rebroadcasting node never vouches for content it did not verify.
    /// Default: false.
    pub rebroadcast_on_merge: bool,
}

#[cfg(feature = "p2p")]
impl Default for P2PConfig {
    fn default() -> Self {
        Self {
            port: None,
            bind_addr: None,
            relay_mode: p2p::iroh::IrohRelayModeConfig::default(),
            discovery: p2p::iroh::IrohDiscoveryConfig::default(),
            allowlist: p2p::iroh::IrohAllowlistConfig::default(),
            max_concurrent_multipath_paths: None,
            secret_key_path: None,
            peer_key: PeerKeySource::default(),
            load_persisted_collections: true,
            sync: SyncTuning::default(),
            rebroadcast_on_merge: false,
        }
    }
}

/// Node assembly configuration: what [`crate::NodeBuilder`]'s setters fill in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeConfig {
    /// Optional application-owned keyring for the transport identity.
    pub peer_keyring: Option<crate::PeerKeyring>,
    pub persistence: Persistence,
    pub transport: TransportConfig,
    /// The network runtime the node's networking runs on: its transport,
    /// its database's network work and the pool's stuck-poll monitor. A node
    /// with a transport that names none runs them on the process's own
    /// ([`edge::on_demand`]); a node without networking starts none.
    pub edge: Option<edge::Edge>,
    pub signing: NodeSigning,
    pub document_acp: DocumentAcpConfig,
    pub query_limits: query::QueryLimits,
    /// Retries of a conflicting transaction; `None` keeps the database default.
    pub max_txn_retries: Option<u32>,
    /// Bytes per block chunk; `None` keeps the database default.
    pub chunk_size: Option<usize>,
    /// Enable node access control at startup, owned by the node identity.
    pub node_acp_enabled: bool,
    /// Per-query deadline; `None` keeps the engine default.
    pub query_timeout: Option<Duration>,
    /// The introspection cache; `None` keeps it on.
    pub introspection_cache: Option<bool>,
    /// The retention floor replicated documents are judged against at
    /// merge; `None` judges nothing.
    pub retention: Option<db::retention::StaticRetentionRule>,
    /// The embedder's check on replicated documents; `None` keeps them all.
    pub replication_filter: Option<ReplicationFilterHandle>,
}

/// An embedder's [`db::replication_filter::ReplicationFilter`], compared by
/// identity.
#[derive(Clone)]
pub struct ReplicationFilterHandle(pub Arc<dyn db::replication_filter::ReplicationFilter>);

impl std::fmt::Debug for ReplicationFilterHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReplicationFilterHandle")
            .finish_non_exhaustive()
    }
}

impl PartialEq for ReplicationFilterHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
