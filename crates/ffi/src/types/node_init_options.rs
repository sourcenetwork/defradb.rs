use std::ffi::{c_char, c_int};
use std::ptr;

/// Options for node initialization.
///
/// Go's NodeInitOptions struct, field for field: an all-zero struct (null
/// strings, zero numbers) is the default node, and every opt-out is a
/// `disable_` flag. The fields after Go's are this implementation's own, so
/// the struct is larger than Go's: a caller passes this header's struct,
/// never one laid out by Go's `defra_structs.h`.
///
/// Go's Badger options name the store regolith replaced, so they are named
/// for the store here (`store_file_size`, `store_encryption_key`). A Go
/// option this implementation does not support yet fails `defra_node_new`
/// when it is set, rather than being ignored.
#[repr(C)]
pub struct NodeInitOptions {
    /// Path to the store directory. Null or empty keeps the store in memory.
    pub db_path: *const c_char,
    /// Comma-separated P2P listen multiaddrs. Empty listens on
    /// `/ip4/0.0.0.0/tcp/9171`, as Go does.
    pub listening_addresses: *const c_char,
    /// Comma-separated seconds between replicator retries, the last one
    /// repeating. Empty keeps the default ladder.
    pub replicator_retry_intervals: *const c_char,
    /// Comma-separated multiaddrs of peers to connect to once started.
    pub peers: *const c_char,
    /// The node's identity, from `defra_identity_new` (0 for none).
    pub identity_ptr: usize,
    /// Keep the store in memory (1=true, 0=false).
    pub in_memory: c_int,
    /// Run without P2P (1=true, 0=false).
    pub disable_p2p: c_int,
    /// Run without the HTTP API (1=true, 0=false). An embedded node does not
    /// serve the HTTP API yet.
    pub disable_api: c_int,
    /// Enable node access control at startup, owned by the node identity
    /// (1=true, 0=false).
    pub enable_node_acp: c_int,
    /// Retries of a conflicting transaction. 0 keeps the default.
    pub max_transaction_retries: c_int,

    /// Store type: "regolith" (default) or "memory". Go's store name,
    /// "badger", means "regolith".
    pub store_type: *const c_char,
    /// The store's compaction output target in bytes (Go's Badger value log
    /// file size). 0 keeps the default.
    pub store_file_size: i64,
    /// 32-byte key encrypting the store at rest, or null.
    pub store_encryption_key: *const u8,
    /// Length of `store_encryption_key` in bytes.
    pub store_encryption_key_len: c_int,

    /// Sign blocks (1=true, 0=false), with the node identity, or with a
    /// generated secp256k1 key when there is none.
    pub enable_signing: c_int,
    /// 32-byte searchable encryption key, or null.
    pub searchable_encryption_key: *const u8,
    /// Length of `searchable_encryption_key` in bytes.
    pub searchable_encryption_key_len: c_int,
    /// Milliseconds a P2P block sync may take. 0 keeps the default.
    pub p2p_block_sync_timeout_ms: i64,
    /// Lens runtime pool size. Not supported yet: must be 0.
    pub lens_pool_size: c_int,
    /// Bytes per block chunk. 0 keeps the default.
    pub chunk_size: c_int,

    /// Run GossipSub (1=true, 0=false).
    pub enable_pub_sub: c_int,
    /// Run the relay client transport (1=true, 0=false).
    pub enable_relay: c_int,
    /// Clear dial backoff before retrying a replicator. Not supported yet:
    /// must be 0.
    pub enable_clear_backoff_on_retry: c_int,
    /// The libp2p Ed25519 private key, or null for the node's stored key.
    pub p2p_private_key: *const u8,
    /// Length of `p2p_private_key` in bytes.
    pub p2p_private_key_len: c_int,

    /// HTTP API address. Not supported yet: must be null or empty.
    pub http_address: *const c_char,
    /// Comma-separated allowed CORS origins. Not supported yet.
    pub http_allowed_origins: *const c_char,
    /// TLS certificate path. Not supported yet.
    pub tls_cert_path: *const c_char,
    /// TLS key path. Not supported yet.
    pub tls_key_path: *const c_char,
    /// HTTP read timeout in milliseconds. Not supported yet.
    pub http_read_timeout_ms: i64,
    /// HTTP write timeout in milliseconds. Not supported yet.
    pub http_write_timeout_ms: i64,
    /// HTTP idle timeout in milliseconds. Not supported yet.
    pub http_idle_timeout_ms: i64,

    /// Document ACP: "local" (default) or "remote" (Vera, configured by the
    /// `remote_dac_` and `vera_` fields).
    pub document_acp_type: *const c_char,
    /// Separate document ACP store path. Not supported: document ACP lives
    /// in the node's store.
    pub document_acp_path: *const c_char,
    /// SourceHub log ID. Not supported: remote document ACP is Vera.
    pub remote_dac_log_id: *const c_char,
    /// Vera gRPC/LCD address.
    pub remote_dac_grpc_address: *const c_char,
    /// Vera CometBFT RPC address.
    pub remote_dac_comet_rpc_address: *const c_char,

    /// Separate node ACP store path. Not supported: node ACP lives in the
    /// node's store.
    pub node_acp_path: *const c_char,

    /// Vera chain ID (e.g., "vera-test").
    pub vera_chain_id: *const c_char,
    /// Vera secp256k1 signer key bytes (raw 32-byte private key).
    pub vera_signer_key: *const u8,
    /// Length of `vera_signer_key` in bytes.
    pub vera_signer_key_len: c_int,
    /// P2P transport: "libp2p" (default) or "iroh".
    pub p2p_transport: *const c_char,
    /// Optional iroh relay URL.
    pub iroh_relay_url: *const c_char,
    /// Optional iroh relay mode string: "default", "disabled", or "custom".
    pub iroh_relay_mode: *const c_char,
    /// Optional JSON array of relay URLs for custom relay mode.
    pub iroh_relay_urls_json: *const c_char,
    /// Optional iroh bind address (e.g. "127.0.0.1").
    pub iroh_bind_addr: *const c_char,
    /// Optional iroh bind UDP port. 0 means OS-assigned/unspecified.
    pub iroh_bind_port: u16,
    /// Run iroh without discovery (1=true, 0=false).
    pub iroh_disable_discovery: c_int,
    /// Optional custom DNS origin domain for iroh discovery.
    pub iroh_discovery_origin_domain: *const c_char,
    /// Optional custom pkarr relay URL for iroh discovery publishing.
    pub iroh_pkarr_relay_url: *const c_char,
    /// Iroh key file from before the node shared one peer key across transports.
    /// Imported as the node's peer key when the store has none; never written.
    pub iroh_key_path: *const c_char,
    /// Maximum concurrent DAG fetch tasks. 0 means use default.
    pub max_concurrent_dag_fetches: usize,
    /// Maximum concurrent push tasks. 0 means use default.
    pub max_concurrent_push_tasks: usize,
    /// Maximum DocSync request document IDs. 0 means use default (1000).
    pub max_doc_sync_request_doc_ids: usize,
    /// Per-peer rate limit burst capacity. 0 means use default (500).
    pub rate_limit_burst: u32,
    /// Per-peer rate limit refill rate (tokens/sec). 0 means use default (50).
    pub rate_limit_rate: f64,
    /// Optional replication filter, asked whether to keep each replicated
    /// document before its commit stores anything; a
    /// `DefraReplicationFilterCallback`, spelled out so C sees a nullable
    /// function pointer. Null keeps everything.
    pub replication_filter: Option<
        unsafe extern "C" fn(
            context: usize,
            collection_id: *const c_char,
            collection_name: *const c_char,
            doc_id: *const c_char,
            fields_json: *const c_char,
        ) -> c_int,
    >,
    /// Passed back to `replication_filter` on every call.
    pub replication_filter_context: usize,
    /// libp2p resource manager memory budget in MiB; zero keeps the default.
    pub p2p_resource_memory_mib: u64,
    /// libp2p resource manager file-descriptor budget, used only together
    /// with `p2p_resource_memory_mib`; zero keeps the default.
    pub p2p_resource_file_descriptors: u64,
    /// Streams one peer may hold open; zero keeps the default.
    pub p2p_max_streams_per_peer: usize,
}

impl Default for NodeInitOptions {
    /// The all-zero struct a C caller gets from `NodeInitOptions opts = {0}`.
    fn default() -> Self {
        Self {
            db_path: ptr::null(),
            listening_addresses: ptr::null(),
            replicator_retry_intervals: ptr::null(),
            peers: ptr::null(),
            identity_ptr: 0,
            in_memory: 0,
            disable_p2p: 0,
            disable_api: 0,
            enable_node_acp: 0,
            max_transaction_retries: 0,
            store_type: ptr::null(),
            store_file_size: 0,
            store_encryption_key: ptr::null(),
            store_encryption_key_len: 0,
            enable_signing: 0,
            searchable_encryption_key: ptr::null(),
            searchable_encryption_key_len: 0,
            p2p_block_sync_timeout_ms: 0,
            lens_pool_size: 0,
            chunk_size: 0,
            enable_pub_sub: 0,
            enable_relay: 0,
            enable_clear_backoff_on_retry: 0,
            p2p_private_key: ptr::null(),
            p2p_private_key_len: 0,
            http_address: ptr::null(),
            http_allowed_origins: ptr::null(),
            tls_cert_path: ptr::null(),
            tls_key_path: ptr::null(),
            http_read_timeout_ms: 0,
            http_write_timeout_ms: 0,
            http_idle_timeout_ms: 0,
            document_acp_type: ptr::null(),
            document_acp_path: ptr::null(),
            remote_dac_log_id: ptr::null(),
            remote_dac_grpc_address: ptr::null(),
            remote_dac_comet_rpc_address: ptr::null(),
            node_acp_path: ptr::null(),
            vera_chain_id: ptr::null(),
            vera_signer_key: ptr::null(),
            vera_signer_key_len: 0,
            p2p_transport: ptr::null(),
            iroh_relay_url: ptr::null(),
            iroh_relay_mode: ptr::null(),
            iroh_relay_urls_json: ptr::null(),
            iroh_bind_addr: ptr::null(),
            iroh_bind_port: 0,
            iroh_disable_discovery: 0,
            iroh_discovery_origin_domain: ptr::null(),
            iroh_pkarr_relay_url: ptr::null(),
            iroh_key_path: ptr::null(),
            max_concurrent_dag_fetches: 0,
            max_concurrent_push_tasks: 0,
            max_doc_sync_request_doc_ids: 0,
            rate_limit_burst: 0,
            rate_limit_rate: 0.0,
            replication_filter: None,
            replication_filter_context: 0,
            p2p_resource_memory_mib: 0,
            p2p_resource_file_descriptors: 0,
            p2p_max_streams_per_peer: 0,
        }
    }
}
