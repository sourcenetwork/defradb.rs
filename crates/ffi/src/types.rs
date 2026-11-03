//! FFI type definitions matching Go's cbindings/defra_structs.h

use std::ffi::{c_char, c_int, CStr, CString};
use std::ptr;

/// FFI result type matching Go's Result struct.
///
/// Status codes:
/// - 0: Success
/// - 1: Error (message in error field)
/// - 2: Subscription (ID in value field, not yet implemented)
///
/// The caller owns its strings and frees them with [`defra_result_free`].
#[repr(C)]
pub struct DefraResult {
    /// Status code: 0=success, 1=error, 2=subscription
    pub status: c_int,
    /// Error message (null on success). Freed with the result.
    pub error: *mut c_char,
    /// JSON value (null on error). Freed with the result.
    pub value: *mut c_char,
}

impl DefraResult {
    /// Create a success result with a JSON value.
    ///
    /// If the value contains null bytes, they are replaced with the Unicode
    /// replacement character to avoid panicking at the FFI boundary.
    pub fn success(value: impl Into<String>) -> Self {
        Self {
            status: 0,
            error: ptr::null_mut(),
            value: sanitize_to_cstring(value, "{}").into_raw(),
        }
    }

    /// Create an error result.
    ///
    /// If the message contains null bytes, they are replaced with the Unicode
    /// replacement character to avoid panicking at the FFI boundary.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: 1,
            error: sanitize_to_cstring(message, "unknown error").into_raw(),
            value: ptr::null_mut(),
        }
    }

    /// Create a success result with no value.
    pub fn ok() -> Self {
        Self {
            status: 0,
            error: ptr::null_mut(),
            value: ptr::null_mut(),
        }
    }

    /// Create a subscription result (status=2) with the subscription ID in value.
    pub fn subscription(id: impl Into<String>) -> Self {
        Self {
            status: 2,
            error: ptr::null_mut(),
            value: sanitize_to_cstring(id, "0").into_raw(),
        }
    }
}

/// A call's JSON value, or its error.
impl From<Result<String, String>> for DefraResult {
    fn from(result: Result<String, String>) -> Self {
        match result {
            Ok(json) => Self::success(json),
            Err(error) => Self::error(error),
        }
    }
}

/// A call's success with no value, or its error.
impl From<Result<(), String>> for DefraResult {
    fn from(result: Result<(), String>) -> Self {
        match result {
            Ok(()) => Self::ok(),
            Err(error) => Self::error(error),
        }
    }
}

/// FFI result for node creation, containing a node handle.
///
/// Matches Go's NewNodeResult struct. The caller frees it with
/// [`defra_new_node_result_free`].
#[repr(C)]
pub struct DefraNewNodeResult {
    /// Status code: 0=success, 1=error
    pub status: c_int,
    /// Error message (null on success). Freed with the result.
    pub error: *mut c_char,
    /// Handle to the node (0 on error).
    pub node_ptr: usize,
}

impl DefraNewNodeResult {
    /// Create a success result with a node handle.
    pub fn success(handle: usize) -> Self {
        Self {
            status: 0,
            error: ptr::null_mut(),
            node_ptr: handle,
        }
    }

    /// Create an error result.
    ///
    /// If the message contains null bytes, they are replaced with the Unicode
    /// replacement character to avoid panicking at the FFI boundary.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: 1,
            error: sanitize_to_cstring(message, "unknown error").into_raw(),
            node_ptr: 0,
        }
    }
}

/// FFI result for transaction creation, containing a transaction handle.
///
/// Matches Go's NewTxnResult struct. The caller frees it with
/// [`defra_new_txn_result_free`].
#[repr(C)]
pub struct DefraNewTxnResult {
    /// Status code: 0=success, 1=error
    pub status: c_int,
    /// Error message (null on success). Freed with the result.
    pub error: *mut c_char,
    /// Handle to the transaction (0 on error).
    pub txn_ptr: usize,
}

impl DefraNewTxnResult {
    /// Create a success result with a transaction handle.
    pub fn success(handle: usize) -> Self {
        Self {
            status: 0,
            error: ptr::null_mut(),
            txn_ptr: handle,
        }
    }

    /// Create an error result.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: 1,
            error: sanitize_to_cstring(message, "unknown error").into_raw(),
            txn_ptr: 0,
        }
    }
}

/// FFI result for identity creation, containing an identity handle.
///
/// Matches Go's NewIdentityResult struct. The caller frees it with
/// [`defra_new_identity_result_free`].
#[repr(C)]
pub struct DefraNewIdentityResult {
    /// Status code: 0=success, 1=error
    pub status: c_int,
    /// Error message (null on success). Freed with the result.
    pub error: *mut c_char,
    /// Handle to the identity (0 on error).
    pub identity_ptr: usize,
}

impl DefraNewIdentityResult {
    /// Create a success result with an identity handle.
    pub fn success(handle: usize) -> Self {
        Self {
            status: 0,
            error: ptr::null_mut(),
            identity_ptr: handle,
        }
    }

    /// Create an error result.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: 1,
            error: sanitize_to_cstring(message, "unknown error").into_raw(),
            identity_ptr: 0,
        }
    }
}

/// Which collection a call acts on, and how it signs.
///
/// Matches Go's CollectionOptions struct. Every string is optional: null or
/// empty leaves that criterion out.
#[repr(C)]
pub struct CollectionOptions {
    /// Collection version ID.
    pub version: *const c_char,
    /// Collection ID.
    pub collection_id: *const c_char,
    /// Collection name.
    pub name: *const c_char,
    /// Include inactive collection versions (1=true, 0=false).
    pub get_inactive: c_int,
    /// Block signing for this call: 0 node default, 1 enabled, -1 disabled.
    pub enable_signing: c_int,
}

impl Default for CollectionOptions {
    fn default() -> Self {
        Self {
            version: ptr::null(),
            collection_id: ptr::null(),
            name: ptr::null(),
            get_inactive: 0,
            enable_signing: 0,
        }
    }
}

/// Options for node initialization.
///
/// Go's NodeInitOptions struct, field for field: an all-zero struct (null
/// strings, zero numbers) is the default node, and every opt-out is a
/// `disable_` flag. The fields after Go's are this implementation's own.
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
    pub vera_signer_key_len: usize,
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
        }
    }
}

// Trait for FFI return types that can represent a panic error.
///
/// Used by the `ffi_entry!` macro to convert caught panics into
/// error results instead of unwinding across the FFI boundary.
pub trait FfiPanicResult {
    fn from_panic(msg: String) -> Self;
}

impl FfiPanicResult for DefraResult {
    fn from_panic(msg: String) -> Self {
        DefraResult::error(msg)
    }
}

impl FfiPanicResult for DefraNewNodeResult {
    fn from_panic(msg: String) -> Self {
        DefraNewNodeResult::error(msg)
    }
}

impl FfiPanicResult for DefraNewTxnResult {
    fn from_panic(msg: String) -> Self {
        DefraNewTxnResult::error(msg)
    }
}

impl FfiPanicResult for DefraNewIdentityResult {
    fn from_panic(msg: String) -> Self {
        DefraNewIdentityResult::error(msg)
    }
}

impl FfiPanicResult for () {
    fn from_panic(msg: String) {
        tracing::warn!(%msg, "a void FFI call panicked");
    }
}

/// Convert a string to a CString, sanitizing null bytes.
///
/// If the string contains embedded null bytes, they are replaced with the
/// Unicode replacement character (`\u{FFFD}`) to avoid panicking at the FFI
/// boundary. If sanitization fails, the fallback string is used.
///
/// # Arguments
///
/// * `value` - The string to convert
/// * `fallback` - Fallback string if conversion fails entirely
pub fn sanitize_to_cstring(value: impl Into<String>, fallback: &str) -> CString {
    let s = value.into();
    match CString::new(s.clone()) {
        Ok(cstring) => cstring,
        Err(_) => {
            let sanitized = s.replace('\0', "\u{FFFD}");
            CString::new(sanitized).unwrap_or_else(|_| {
                CString::new(fallback).unwrap_or_else(|_| CString::new("error").unwrap())
            })
        }
    }
}

/// Convert a C string pointer to a Rust String (or None if null).
///
/// # Safety
///
/// The pointer must be null or point to a valid null-terminated string.
pub unsafe fn c_str_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }
}

/// Frees a string this crate allocated and nulls the pointer, so a second
/// free does nothing.
///
/// # Safety
///
/// `ptr` must be null or a string this crate allocated.
unsafe fn free_c_string(ptr: &mut *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(*ptr));
        *ptr = ptr::null_mut();
    }
}

/// Frees the strings of a result and nulls them, so freeing it again does
/// nothing. A null `result` does nothing.
///
/// # Safety
///
/// `result` must be null or point to a result a call returned.
#[no_mangle]
pub unsafe extern "C" fn defra_result_free(result: *mut DefraResult) {
    if let Some(result) = result.as_mut() {
        free_c_string(&mut result.error);
        free_c_string(&mut result.value);
    }
}

/// Frees the error of a node creation result and nulls it, so freeing it
/// again does nothing. The node stays open. A null `result` does nothing.
///
/// # Safety
///
/// `result` must be null or point to a result a call returned.
#[no_mangle]
pub unsafe extern "C" fn defra_new_node_result_free(result: *mut DefraNewNodeResult) {
    if let Some(result) = result.as_mut() {
        free_c_string(&mut result.error);
    }
}

/// Frees the error of a transaction creation result and nulls it, so
/// freeing it again does nothing. The transaction stays open. A null
/// `result` does nothing.
///
/// # Safety
///
/// `result` must be null or point to a result a call returned.
#[no_mangle]
pub unsafe extern "C" fn defra_new_txn_result_free(result: *mut DefraNewTxnResult) {
    if let Some(result) = result.as_mut() {
        free_c_string(&mut result.error);
    }
}

/// Frees the error of an identity creation result and nulls it, so freeing
/// it again does nothing. The identity stays registered. A null `result`
/// does nothing.
///
/// # Safety
///
/// `result` must be null or point to a result a call returned.
#[no_mangle]
pub unsafe extern "C" fn defra_new_identity_result_free(result: *mut DefraNewIdentityResult) {
    if let Some(result) = result.as_mut() {
        free_c_string(&mut result.error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defra_result_success() {
        let mut result = DefraResult::success(r#"{"data": "test"}"#);
        assert_eq!(result.status, 0);
        assert!(result.error.is_null());
        assert!(!result.value.is_null());

        unsafe { defra_result_free(&mut result) };
    }

    #[test]
    fn test_defra_result_error() {
        let mut result = DefraResult::error("something went wrong");
        assert_eq!(result.status, 1);
        assert!(!result.error.is_null());
        assert!(result.value.is_null());

        unsafe { defra_result_free(&mut result) };
    }

    #[test]
    fn test_new_node_result() {
        let result = DefraNewNodeResult::success(42);
        assert_eq!(result.status, 0);
        assert_eq!(result.node_ptr, 42);
        assert!(result.error.is_null());
    }

    // Edge case tests for null byte handling (H2)

    #[test]
    fn test_defra_result_success_with_null_bytes() {
        // String with embedded null byte should not panic
        let value_with_null = "hello\0world";
        let mut result = DefraResult::success(value_with_null);
        assert_eq!(result.status, 0);
        assert!(!result.value.is_null());

        // Value should have null bytes replaced
        let value = unsafe { CStr::from_ptr(result.value).to_string_lossy() };
        assert!(value.contains('\u{FFFD}'), "null byte should be replaced");
        assert!(!value.contains('\0'), "should not contain null byte");

        unsafe { defra_result_free(&mut result) };
    }

    #[test]
    fn test_defra_result_error_with_null_bytes() {
        // Error message with embedded null byte should not panic
        let error_with_null = "error\0message";
        let mut result = DefraResult::error(error_with_null);
        assert_eq!(result.status, 1);
        assert!(!result.error.is_null());

        // Error should have null bytes replaced
        let error = unsafe { CStr::from_ptr(result.error).to_string_lossy() };
        assert!(error.contains('\u{FFFD}'), "null byte should be replaced");
        assert!(!error.contains('\0'), "should not contain null byte");

        unsafe { defra_result_free(&mut result) };
    }

    #[test]
    fn test_new_node_result_error_with_null_bytes() {
        // Error message with embedded null byte should not panic
        let error_with_null = "node\0error";
        let mut result = DefraNewNodeResult::error(error_with_null);
        assert_eq!(result.status, 1);
        assert!(!result.error.is_null());
        assert_eq!(result.node_ptr, 0);

        // Error should have null bytes replaced
        let error = unsafe { CStr::from_ptr(result.error).to_string_lossy() };
        assert!(error.contains('\u{FFFD}'), "null byte should be replaced");

        unsafe { defra_new_node_result_free(&mut result) };
    }

    #[test]
    fn test_c_str_to_string_null_ptr() {
        let result = unsafe { c_str_to_string(ptr::null()) };
        assert!(result.is_none());
    }

    #[test]
    fn freeing_a_result_nulls_its_strings_so_a_second_free_does_nothing() {
        let mut result = DefraResult::error("gone");
        unsafe { defra_result_free(&mut result) };
        assert!(result.error.is_null());
        assert!(result.value.is_null());
        unsafe { defra_result_free(&mut result) };

        let mut node = DefraNewNodeResult::error("gone");
        unsafe { defra_new_node_result_free(&mut node) };
        assert!(node.error.is_null());
        unsafe { defra_new_node_result_free(&mut node) };

        let mut txn = DefraNewTxnResult::error("gone");
        unsafe { defra_new_txn_result_free(&mut txn) };
        assert!(txn.error.is_null());
        unsafe { defra_new_txn_result_free(&mut txn) };

        let mut identity = DefraNewIdentityResult::error("gone");
        unsafe { defra_new_identity_result_free(&mut identity) };
        assert!(identity.error.is_null());
        unsafe { defra_new_identity_result_free(&mut identity) };
    }

    #[test]
    fn freeing_a_null_result_does_nothing() {
        unsafe {
            defra_result_free(ptr::null_mut());
            defra_new_node_result_free(ptr::null_mut());
            defra_new_txn_result_free(ptr::null_mut());
            defra_new_identity_result_free(ptr::null_mut());
        }
    }

    #[test]
    fn test_defra_result_ok() {
        let result = DefraResult::ok();
        assert_eq!(result.status, 0);
        assert!(result.error.is_null());
        assert!(result.value.is_null());
    }
}
