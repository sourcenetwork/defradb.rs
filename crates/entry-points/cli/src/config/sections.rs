//! Configuration section structs

use std::net::SocketAddr;

use db::{DEFAULT_TRANSACTION_CLEANUP_INTERVAL, DEFAULT_TRANSACTION_IDLE_TIMEOUT};
use serde::{Deserialize, Serialize};

use query::{
    DEFAULT_MAX_FILTER_DEPTH, DEFAULT_MAX_FRAGMENT_NESTING, DEFAULT_MAX_QUERY_DEPTH,
    DEFAULT_MAX_QUERY_WIDTH,
};
use storage::backends::DurabilityMode;

use super::types::{
    AcpDocumentType, DatastoreType, KeyringBackend, LogFormat, LogLevel, LogOutput,
};
use crate::error::{Error, Result};

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    pub level: LogLevel,
    pub output: LogOutput,
    pub format: LogFormat,
    pub stacktrace: bool,
    pub source: bool,
    pub color_disabled: bool,
    #[serde(default)]
    pub overrides: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            output: LogOutput::Stderr,
            format: LogFormat::Text,
            stacktrace: false,
            source: false,
            color_disabled: false,
            overrides: String::new(),
        }
    }
}

/// API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiConfig {
    pub address: String,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    #[serde(default)]
    pub pubkey_path: String,
    #[serde(default)]
    pub privkey_path: String,
    /// Max request body size in bytes (0 = unlimited). Default: 0 (no limit).
    #[serde(default)]
    pub max_body_size: u64,
    /// Max schema request body size in bytes (0 = unlimited). Default: 0 (no limit).
    #[serde(default)]
    pub max_schema_size: u64,
    /// Max backup import body size in bytes (0 = unlimited). Default: 100 MiB.
    #[serde(default)]
    pub max_backup_size: u64,
    /// Request timeout in seconds (0 = no timeout). Default: 300 (5 minutes).
    #[serde(default = "default_request_timeout")]
    pub request_timeout: u64,
    /// Bound on waiting for a transaction commit in milliseconds (0 = no
    /// bound). Default: 0.
    #[serde(default = "default_commit_timeout_ms")]
    pub commit_timeout_ms: u64,
    /// Max concurrent requests (0 = unlimited). Default: 1000.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_requests: usize,
    /// Query execution timeout in seconds (0 = no timeout). Default: 30.
    #[serde(default = "default_query_timeout")]
    pub query_timeout: u64,
    /// Max idle age for explicit HTTP transactions in seconds (0 = disabled). Default: 600.
    #[serde(default = "default_transaction_idle_timeout")]
    pub transaction_idle_timeout: u64,
    /// Interval between explicit HTTP transaction cleanup sweeps in seconds. Default: 60.
    #[serde(default = "default_transaction_cleanup_interval")]
    pub transaction_cleanup_interval: u64,
    /// Max GraphQL selection nesting depth (0 = unlimited). Default: 20.
    #[serde(default = "default_query_max_depth")]
    pub query_max_depth: usize,
    /// Max fields at any GraphQL selection level (0 = unlimited). Default: 100.
    #[serde(default = "default_query_max_width")]
    pub query_max_width: usize,
    /// Max recursive filter nesting depth (0 = unlimited). Default: 50.
    #[serde(default = "default_query_max_filter_depth")]
    pub query_max_filter_depth: usize,
    /// Max fragments one spread chain nests, from 1 to what a query thread's
    /// stack holds (checked at startup). Default: 32.
    #[serde(default = "default_query_max_fragment_nesting")]
    pub query_max_fragment_nesting: usize,
    /// Cache the built introspection schema for the committed collection
    /// set, instead of rebuilding it per query. Default: true.
    #[serde(default = "default_introspection_cache")]
    pub introspection_cache: bool,
    /// Postgres wire protocol address (empty = disabled). Default: "" (disabled).
    #[cfg(feature = "postgres")]
    #[serde(default)]
    pub pg_address: String,
}

fn default_request_timeout() -> u64 {
    300
}

fn default_commit_timeout_ms() -> u64 {
    defra_http::DEFAULT_COMMIT_TIMEOUT_MS
}

fn default_max_concurrent() -> usize {
    1000
}

fn default_query_timeout() -> u64 {
    30
}

fn default_transaction_idle_timeout() -> u64 {
    DEFAULT_TRANSACTION_IDLE_TIMEOUT.as_secs()
}

fn default_transaction_cleanup_interval() -> u64 {
    DEFAULT_TRANSACTION_CLEANUP_INTERVAL.as_secs()
}

fn default_query_max_depth() -> usize {
    DEFAULT_MAX_QUERY_DEPTH
}

fn default_query_max_width() -> usize {
    DEFAULT_MAX_QUERY_WIDTH
}

fn default_query_max_filter_depth() -> usize {
    DEFAULT_MAX_FILTER_DEPTH
}

fn default_query_max_fragment_nesting() -> usize {
    DEFAULT_MAX_FRAGMENT_NESTING
}

fn default_introspection_cache() -> bool {
    true
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:9181".to_string(),
            allowed_origins: Vec::new(),
            pubkey_path: String::new(),
            privkey_path: String::new(),
            max_body_size: 0,
            max_schema_size: 0,
            max_backup_size: defra_http::DEFAULT_MAX_BACKUP_SIZE,
            request_timeout: default_request_timeout(),
            commit_timeout_ms: default_commit_timeout_ms(),
            max_concurrent_requests: default_max_concurrent(),
            query_timeout: default_query_timeout(),
            transaction_idle_timeout: default_transaction_idle_timeout(),
            transaction_cleanup_interval: default_transaction_cleanup_interval(),
            query_max_depth: default_query_max_depth(),
            query_max_width: default_query_max_width(),
            query_max_filter_depth: default_query_max_filter_depth(),
            query_max_fragment_nesting: default_query_max_fragment_nesting(),
            introspection_cache: default_introspection_cache(),
            #[cfg(feature = "postgres")]
            pg_address: String::new(),
        }
    }
}

impl ApiConfig {
    /// Validate the API configuration
    pub fn validate(&self) -> Result<()> {
        self.address
            .parse::<SocketAddr>()
            .map_err(|e| Error::InvalidApiAddress(self.address.clone(), e.to_string()))?;

        let has_pub = !self.pubkey_path.is_empty();
        let has_priv = !self.privkey_path.is_empty();
        if has_pub != has_priv {
            return Err(Error::IncompleteTlsConfig);
        }

        query::QueryLimits {
            max_fragment_nesting: self.query_max_fragment_nesting,
            ..query::QueryLimits::default()
        }
        .check_fragment_nesting()
        .map_err(|e| Error::InvalidConfig(e.to_string()))?;

        if self.transaction_idle_timeout > 0 && self.transaction_cleanup_interval == 0 {
            return Err(Error::InvalidConfig(
                "api.transaction_cleanup_interval must be > 0 when transaction_idle_timeout is enabled"
                    .to_string(),
            ));
        }

        Ok(())
    }

    /// Check if TLS is enabled (both pubkey_path and privkey_path are configured)
    pub fn tls_enabled(&self) -> bool {
        !self.pubkey_path.is_empty() && !self.privkey_path.is_empty()
    }
}

/// Embedding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_embedding_api_key_env")]
    pub api_key_env: String,
}

fn default_embedding_api_key_env() -> String {
    "OPENAI_API_KEY".to_string()
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            model: String::new(),
            api_key_env: default_embedding_api_key_env(),
        }
    }
}

/// Datastore configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatastoreConfig {
    pub store: DatastoreType,
    pub path: String,
    pub max_txn_retries: u32,
    /// Datastore value log file size in bytes. Set only by `--valuelogfilesize`.
    ///
    /// `None` means unset: each backend keeps its own default. Deliberately not
    /// defaulted to Go's 1 GiB -- that is Badger's own default, so passing it
    /// through is a no-op in Go, whereas regolith defaults its
    /// compaction output target to 64 MiB and would be moved 16x by it.
    ///
    /// `skip` rather than `default`, because the old `u64` field carried that
    /// 1 GiB default and `create_if_missing` serialized it into every generated
    /// `config.yaml`. Deserializing it back would apply the 16x change to any
    /// node that had ever been started, so a persisted value must not be
    /// mistaken for an operator having asked for it. Skipping also stops the
    /// key being written again, so the ambiguity cannot recur.
    #[serde(skip)]
    pub valuelogfilesize: Option<u64>,
    pub no_encryption: bool,
    pub no_searchable_encryption: bool,
    pub no_signing: bool,
    pub default_key_type: String,
    /// Durability mode for the storage backend.
    ///
    /// - `immediate` (default): fsync on every commit, safe against OS crashes
    /// - `eventual`: defer fsync to OS for higher write throughput
    #[serde(default)]
    pub durability: DurabilityMode,
    /// DAG traversal depth limit; longer document histories resume in bounded turns.
    #[serde(default = "default_max_merge_depth")]
    pub max_merge_depth: usize,
    /// Enable engine file encryption at rest for the storage backend.
    ///
    /// When enabled, all stored values are encrypted with AES-256-GCM keyed by
    /// the keyring `encryption-key` (generated on first use if absent). Keys
    /// remain plaintext to preserve prefix/range iteration. Default: false.
    /// Once enabled for a store, it must stay enabled: reading without the
    /// matching key fails loudly rather than returning garbage.
    #[serde(default)]
    pub at_rest_encryption: bool,
}

fn default_max_merge_depth() -> usize {
    db::merge::DEFAULT_MAX_MERGE_DEPTH
}

impl Default for DatastoreConfig {
    fn default() -> Self {
        Self {
            store: DatastoreType::Regolith,
            path: "data".to_string(),
            max_txn_retries: 5,
            valuelogfilesize: None,
            no_encryption: false,
            no_searchable_encryption: false,
            no_signing: false,
            default_key_type: "secp256k1".to_string(),
            durability: DurabilityMode::default(),
            max_merge_depth: default_max_merge_depth(),
            at_rest_encryption: false,
        }
    }
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetConfig {
    pub p2p_disabled: bool,
    /// Peers dialed at start, each `<peer id>@<udp address>`.
    #[serde(default)]
    pub peers: Vec<String>,
    /// The IP address the pinc endpoint binds. Unset binds loopback
    /// (127.0.0.1), as Go's default `net.p2paddresses` does, so a node
    /// started with no P2P settings takes no connection from another host.
    /// A wildcard (`0.0.0.0`, `::`) listens on every interface and lists
    /// each interface's address, never loopback.
    #[serde(default)]
    pub pinc_bind_addr: Option<std::net::IpAddr>,
    /// The UDP port the pinc endpoint binds. Unset lets the OS pick one.
    #[serde(default)]
    pub pinc_bind_port: Option<u16>,
    /// Addresses peers reach the node at that are on none of its
    /// interfaces, each `<ip>:<port>` (a public address in front of a NAT,
    /// a forwarded port). The node lists them first.
    #[serde(default)]
    pub pinc_public_addrs: Vec<std::net::SocketAddr>,
    /// Peer ids allowed to open an inbound connection. Empty (the default)
    /// accepts every peer, which is fine on a private network. Listing any id
    /// switches the node to accepting only the ids listed, which is what a
    /// node reachable from the internet needs: a topic is named by a
    /// content-derived collection id, identical for anyone holding the same
    /// schema, so an open node hands its documents to any peer that can name
    /// a collection.
    #[serde(default)]
    pub pinc_allowed_peers: Vec<String>,
    /// The pinc hubs topics run through, as `<peer id>@<udp address>`, at
    /// most four. Topic broadcasts and the KMS key exchange reach peers only
    /// through a hub, so a node that names none syncs only over direct
    /// requests.
    #[serde(default)]
    pub pinc_hubs: Vec<String>,
    /// Per-peer rate limit burst capacity (max tokens in bucket). Default: 500.
    #[serde(default = "default_rate_limit_burst")]
    pub p2p_rate_limit_burst: u32,
    /// Per-peer rate limit refill rate (tokens per second). Default: 50.
    #[serde(default = "default_rate_limit_rate")]
    pub p2p_rate_limit_rate: f64,
    /// Max document IDs accepted in one DocSync request. Default: 1000.
    #[serde(default = "default_max_doc_sync_request_doc_ids")]
    pub p2p_max_doc_sync_request_doc_ids: usize,
    /// Max pending-DAG registrations held while Bitswap completes missing
    /// links; overflow is nacked back to the pusher. Each source peer may use
    /// at most one quarter of this capacity. Default: 1000.
    #[serde(default = "default_max_pending_dags")]
    pub p2p_max_pending_dags: usize,
    /// Max concurrent DAG fetches across all peers. Default: 4.
    #[serde(default = "default_max_concurrent_dag_fetches")]
    pub p2p_max_concurrent_dag_fetches: usize,
    /// Seconds an unacknowledged pending-DAG entry lives before eviction; an
    /// acknowledged entry does not expire. Default: 300.
    #[serde(default = "default_pending_dag_ttl_secs")]
    pub p2p_pending_dag_ttl_secs: u64,
    /// Re-announce blocks merged from peers on their gossip topics, so
    /// subscribers with no transport route to the author still receive them
    /// through whichever peer merged them. Signatures are verified on merge,
    /// so a rebroadcasting node never vouches for content it did not verify.
    /// Default: false.
    #[serde(default)]
    pub p2p_rebroadcast_on_merge: bool,
    /// Max queued outbound push jobs; overflow defers to the persisted retry
    /// ladder. Default: 1024.
    #[serde(default = "default_push_queue_capacity")]
    pub p2p_push_queue_capacity: usize,
    /// Max resident bytes across queued outbound push jobs. Default: 32 MiB.
    #[serde(default = "default_push_queue_byte_capacity")]
    pub p2p_push_queue_byte_capacity: usize,
    /// Max outbound push jobs concurrently in flight to one peer. Default: 4.
    #[serde(default = "default_max_active_pushes_per_peer")]
    pub p2p_max_active_pushes_per_peer: usize,
}

fn default_rate_limit_burst() -> u32 {
    500
}
fn default_rate_limit_rate() -> f64 {
    50.0
}
fn default_max_doc_sync_request_doc_ids() -> usize {
    p2p::sync::DEFAULT_MAX_DOC_SYNC_REQUEST_DOC_IDS
}
fn default_max_pending_dags() -> usize {
    p2p::sync::DEFAULT_MAX_PENDING_DAGS
}
fn default_max_concurrent_dag_fetches() -> usize {
    p2p::sync::DEFAULT_MAX_CONCURRENT_DAG_FETCHES
}
fn default_pending_dag_ttl_secs() -> u64 {
    p2p::sync::PENDING_DAG_TTL.as_secs()
}
fn default_push_queue_capacity() -> usize {
    p2p::sync::DEFAULT_PUSH_QUEUE_CAPACITY
}
fn default_push_queue_byte_capacity() -> usize {
    p2p::sync::DEFAULT_PUSH_QUEUE_BYTE_CAPACITY
}
fn default_max_active_pushes_per_peer() -> usize {
    p2p::sync::DEFAULT_MAX_ACTIVE_PUSHES_PER_PEER
}
impl Default for NetConfig {
    fn default() -> Self {
        Self {
            p2p_disabled: false,
            peers: Vec::new(),
            pinc_bind_addr: None,
            pinc_bind_port: None,
            pinc_public_addrs: Vec::new(),
            pinc_allowed_peers: Vec::new(),
            pinc_hubs: Vec::new(),
            p2p_rate_limit_burst: default_rate_limit_burst(),
            p2p_rate_limit_rate: default_rate_limit_rate(),
            p2p_max_doc_sync_request_doc_ids: default_max_doc_sync_request_doc_ids(),
            p2p_max_pending_dags: default_max_pending_dags(),
            p2p_max_concurrent_dag_fetches: default_max_concurrent_dag_fetches(),
            p2p_pending_dag_ttl_secs: default_pending_dag_ttl_secs(),
            p2p_rebroadcast_on_merge: false,
            p2p_push_queue_capacity: default_push_queue_capacity(),
            p2p_push_queue_byte_capacity: default_push_queue_byte_capacity(),
            p2p_max_active_pushes_per_peer: default_max_active_pushes_per_peer(),
        }
    }
}

impl NetConfig {
    /// The address the pinc endpoint binds: `pinc_bind_addr`, loopback when
    /// unset, on `pinc_bind_port`, a port the OS picks when unset.
    pub fn pinc_bind(&self) -> std::net::SocketAddr {
        std::net::SocketAddr::new(
            self.pinc_bind_addr
                .unwrap_or(std::net::IpAddr::from([127, 0, 0, 1])),
            self.pinc_bind_port.unwrap_or(0),
        )
    }

    /// Validate the network configuration
    pub fn validate(&self) -> Result<()> {
        if self.p2p_disabled {
            return Ok(());
        }
        for addr in self.peers.iter().chain(&self.pinc_hubs) {
            defra_pinc::parse_public_peer_addr(addr)
                .map_err(|e| Error::InvalidConfig(format!("invalid pinc address {addr}: {e}")))?;
        }
        Ok(())
    }
}

/// Keyring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyringConfig {
    pub backend: KeyringBackend,
    pub path: String,
    pub namespace: String,
    pub disabled: bool,
}

impl Default for KeyringConfig {
    fn default() -> Self {
        Self {
            backend: KeyringBackend::File,
            path: "keys".to_string(),
            namespace: "defradb".to_string(),
            disabled: false,
        }
    }
}

/// Access Control Policy (ACP) configuration.
///
/// ACP provides two levels of access control:
/// - Node Access Control (NAC): Controls access to node-level operations
/// - Document Access Control (DAC): Controls access to individual documents
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AcpConfig {
    /// Enable Node Access Control (NAC).
    ///
    /// When enabled, node operations require authentication and authorization.
    /// Default: false (all operations allowed without authentication)
    pub node_enable: bool,

    /// Document ACP type.
    ///
    /// - `none`: No document-level access control (default)
    /// - `local`: Local Zanzibar-based access control
    /// - `vera`: Remote Vera access control
    pub document_type: AcpDocumentType,

    /// Vera LCD endpoint (e.g., "http://localhost:1317")
    #[cfg(feature = "vera")]
    #[serde(default, alias = "sourcehub_address")]
    pub vera_address: String,

    /// Vera gRPC endpoint (e.g., "http://localhost:9090")
    #[cfg(feature = "vera")]
    #[serde(default, alias = "sourcehub_grpc_address")]
    pub vera_grpc_address: String,

    /// Vera CometBFT RPC endpoint (e.g., "http://localhost:26657")
    #[cfg(feature = "vera")]
    #[serde(default, alias = "sourcehub_comet_address")]
    pub vera_comet_address: String,

    /// Vera CometBFT WebSocket endpoint for ACP cache invalidation.
    #[cfg(feature = "vera")]
    #[serde(default, alias = "sourcehub_events_ws")]
    pub vera_events_ws: String,

    /// Vera chain ID (e.g., "vera-test")
    #[cfg(feature = "vera")]
    #[serde(default, alias = "sourcehub_chain_id")]
    pub vera_chain_id: String,

    /// vera.rs JSON-RPC endpoint (e.g., "http://localhost:8545")
    #[cfg(feature = "vera")]
    #[serde(default)]
    pub vera_rs_address: String,

    /// Trusted Vera consensus public key from operator configuration (hex).
    #[cfg(feature = "vera")]
    #[serde(default)]
    pub vera_consensus_key: String,

    /// Identifier of the Vera deployment's genesis block from operator
    /// configuration (hex). With it the node follows the deployment when
    /// another of the account's devices becomes the active one; without it
    /// every term above 0 is refused and, after such a switch, every verified
    /// read fails until it is set.
    #[cfg(feature = "vera")]
    #[serde(default)]
    pub vera_genesis_id: String,

    /// Vera deployment identifier used to bind native submissions.
    #[cfg(feature = "vera")]
    #[serde(default)]
    pub vera_deployment_id: Option<u64>,

    /// Operator keys of the embedded Vera deployment: lowercase hexadecimal
    /// compressed secp256k1 keys, and `did:key` for Ed25519 and P-256 keys.
    /// Empty: the node identity is the only operator.
    #[cfg(feature = "embedded-vera")]
    #[serde(default)]
    pub vera_embedded_operators: Vec<String>,

    /// How many distinct operator approvals an administrative change of the
    /// embedded Vera deployment needs. Unset: one.
    #[cfg(feature = "embedded-vera")]
    #[serde(default)]
    pub vera_embedded_threshold: Option<u16>,

    /// Consensus listener of the embedded Vera node. The default keeps it on
    /// loopback with a port the system picks.
    #[cfg(feature = "embedded-vera")]
    #[serde(default = "default_vera_embedded_listen")]
    pub vera_embedded_listen: String,

    /// Account the client names as the token's authorized account when it
    /// talks to a node using remote document ACP (Go: `--remote-dac-address`).
    #[serde(default)]
    pub remote_address: String,

    /// Circuit breaker failure threshold before tripping. Default: 3.
    #[serde(default = "default_acp_cb_threshold")]
    pub circuit_breaker_threshold: u32,

    /// Circuit breaker reset timeout in seconds. Default: 30.
    #[serde(default = "default_acp_cb_reset_timeout")]
    pub circuit_breaker_reset_timeout: u64,

    /// Request timeout in seconds for Vera/vera.rs network calls. Default: 5.
    #[serde(default = "default_acp_request_timeout")]
    pub request_timeout: u64,

    /// Policy cache TTL in seconds. Default: 300.
    #[serde(default = "default_acp_cache_ttl")]
    pub cache_ttl: u64,

    /// Receipt polling timeout in seconds for vera.rs transactions. Default: 30.
    #[serde(default = "default_acp_receipt_timeout")]
    pub receipt_timeout: u64,
}

fn default_acp_cb_threshold() -> u32 {
    3
}
fn default_acp_cb_reset_timeout() -> u64 {
    30
}
fn default_acp_request_timeout() -> u64 {
    5
}
fn default_acp_cache_ttl() -> u64 {
    300
}
fn default_acp_receipt_timeout() -> u64 {
    30
}
#[cfg(feature = "embedded-vera")]
fn default_vera_embedded_listen() -> String {
    "127.0.0.1:0".to_string()
}

impl Default for AcpConfig {
    fn default() -> Self {
        Self {
            node_enable: false,
            document_type: AcpDocumentType::None,
            #[cfg(feature = "vera")]
            vera_address: String::new(),
            #[cfg(feature = "vera")]
            vera_grpc_address: String::new(),
            #[cfg(feature = "vera")]
            vera_comet_address: String::new(),
            #[cfg(feature = "vera")]
            vera_events_ws: String::new(),
            #[cfg(feature = "vera")]
            vera_chain_id: String::new(),
            #[cfg(feature = "vera")]
            vera_rs_address: String::new(),
            #[cfg(feature = "vera")]
            vera_consensus_key: String::new(),
            #[cfg(feature = "vera")]
            vera_genesis_id: String::new(),
            #[cfg(feature = "vera")]
            vera_deployment_id: None,
            #[cfg(feature = "embedded-vera")]
            vera_embedded_operators: Vec::new(),
            #[cfg(feature = "embedded-vera")]
            vera_embedded_threshold: None,
            #[cfg(feature = "embedded-vera")]
            vera_embedded_listen: default_vera_embedded_listen(),
            remote_address: String::new(),
            circuit_breaker_threshold: default_acp_cb_threshold(),
            circuit_breaker_reset_timeout: default_acp_cb_reset_timeout(),
            request_timeout: default_acp_request_timeout(),
            cache_ttl: default_acp_cache_ttl(),
            receipt_timeout: default_acp_receipt_timeout(),
        }
    }
}

impl AcpConfig {
    /// Validate ACP tuning parameters.
    pub fn validate(&self) -> Result<()> {
        if self.circuit_breaker_threshold == 0 {
            return Err(Error::InvalidConfig(
                "acp_circuit_breaker_threshold must be > 0: \
                 a zero threshold means the circuit breaker trips immediately"
                    .into(),
            ));
        }
        if self.circuit_breaker_reset_timeout == 0 {
            return Err(Error::InvalidConfig(
                "acp_circuit_breaker_reset_timeout must be > 0: \
                 a zero reset timeout means the circuit breaker never recovers"
                    .into(),
            ));
        }
        if self.request_timeout == 0 {
            return Err(Error::InvalidConfig(
                "acp_request_timeout must be > 0: \
                 a zero timeout disables the deadline and requests may hang indefinitely"
                    .into(),
            ));
        }
        if self.cache_ttl == 0 {
            return Err(Error::InvalidConfig(
                "acp_cache_ttl must be > 0: \
                 a zero TTL means every policy lookup bypasses the cache entirely"
                    .into(),
            ));
        }
        if self.receipt_timeout == 0 {
            return Err(Error::InvalidConfig(
                "acp_receipt_timeout must be > 0: \
                 a zero timeout means vera.rs transaction receipts are never awaited"
                    .into(),
            ));
        }
        Ok(())
    }
}
