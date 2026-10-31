//! FFI type definitions matching Go's cbindings/defra_structs.h

use std::ffi::{c_char, c_int, CStr, CString};
use std::ptr;

/// FFI result type matching Go's Result struct.
///
/// Status codes:
/// - 0: Success
/// - 1: Error (message in error field)
/// - 2: Subscription (ID in value field, not yet implemented)
#[repr(C)]
pub struct FfiResult {
    /// Status code: 0=success, 1=error, 2=subscription
    pub status: c_int,
    /// Error message (null on success). Caller must free with `defra_free_string`.
    pub error: *mut c_char,
    /// JSON value (null on error). Caller must free with `defra_free_string`.
    pub value: *mut c_char,
}

impl FfiResult {
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

    /// The text of an error result, freeing the result; `None` when it is not
    /// an error.
    pub(crate) fn into_error(self) -> Option<String> {
        if !self.value.is_null() {
            // SAFETY: `value` came from `CString::into_raw` and is freed once.
            drop(unsafe { std::ffi::CString::from_raw(self.value) });
        }
        if self.error.is_null() {
            return None;
        }
        // SAFETY: `error` came from `CString::into_raw` and is freed once.
        let error = unsafe { std::ffi::CString::from_raw(self.error) };
        Some(error.to_string_lossy().into_owned())
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
impl From<Result<String, String>> for FfiResult {
    fn from(result: Result<String, String>) -> Self {
        match result {
            Ok(json) => Self::success(json),
            Err(error) => Self::error(error),
        }
    }
}

/// A call's success with no value, or its error.
impl From<Result<(), String>> for FfiResult {
    fn from(result: Result<(), String>) -> Self {
        match result {
            Ok(()) => Self::ok(),
            Err(error) => Self::error(error),
        }
    }
}

/// FFI result for node creation, containing a node handle.
///
/// Matches Go's NewNodeResult struct.
#[repr(C)]
pub struct NewNodeResult {
    /// Status code: 0=success, 1=error
    pub status: c_int,
    /// Error message (null on success). Caller must free with `defra_free_string`.
    pub error: *mut c_char,
    /// Handle to the node (0 on error).
    pub node_ptr: usize,
}

impl NewNodeResult {
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

/// FFI result for transaction creation, containing a transaction ID.
#[repr(C)]
pub struct NewTxnResult {
    /// Status code: 0=success, 1=error
    pub status: c_int,
    /// Error message (null on success). Caller must free with `defra_free_string`.
    pub error: *mut c_char,
    /// Transaction ID (null on error). Caller must free with `defra_free_string`.
    pub txn_id: *mut c_char,
}

impl NewTxnResult {
    /// Create a success result with a transaction ID.
    pub fn success(txn_id: impl Into<String>) -> Self {
        Self {
            status: 0,
            error: ptr::null_mut(),
            txn_id: sanitize_to_cstring(txn_id, "unknown").into_raw(),
        }
    }

    /// Create an error result.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: 1,
            error: sanitize_to_cstring(message, "unknown error").into_raw(),
            txn_id: ptr::null_mut(),
        }
    }
}

/// Options for node initialization.
///
/// Matches Go's NodeInitOptions struct.
#[repr(C)]
pub struct NodeInitOptions {
    /// Path to store directory (null for in-memory).
    pub db_path: *const c_char,
    /// Use in-memory storage (1=true, 0=false).
    pub in_memory: c_int,
    /// Storage backend name: "regolith" (default) or "memory".
    /// Null uses "regolith" for persistent or "memory" for in-memory.
    pub datastore_backend: *const c_char,
    /// Enable block signing (1=true, 0=false).
    /// When enabled, the node uses a signing key for block signatures.
    /// If signing_private_key is provided, that key is used.
    /// Otherwise, a random secp256k1 key pair is generated.
    pub enable_signing: c_int,
    /// Optional: signing key type string (e.g. "secp256k1", "secp256r1", "ed25519").
    /// Only used when signing_private_key is provided.
    pub signing_key_type: *const c_char,
    /// Optional: raw private key bytes used for the node identity.
    /// Block signing remains controlled by enable_signing.
    pub signing_private_key: *const u8,
    /// Length of signing_private_key in bytes. 0 if null.
    pub signing_private_key_len: usize,
    /// Vera gRPC/LCD address (null = use local ACP).
    pub vera_grpc_address: *const c_char,
    /// Vera CometBFT RPC address.
    pub vera_comet_rpc_address: *const c_char,
    /// Vera chain ID (e.g., "vera-test").
    pub vera_chain_id: *const c_char,
    /// Vera secp256k1 signer key bytes (raw 32-byte private key).
    pub vera_signer_key: *const u8,
    /// Length of vera_signer_key. 0 if null.
    pub vera_signer_key_len: usize,
    /// Optional P2P transport backend for `new_node_with_p2p`: "libp2p" (default) or "iroh".
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
    /// Enable iroh discovery (1=true, 0=false). Default true.
    pub iroh_discovery: c_int,
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
    /// Speak Go Shinzo's P2P wire on the libp2p transport (1=true, 0=false).
    pub shinzo_wire: c_int,
    /// Optional retention rule as JSON: each collection ID it covers, with
    /// the integer field that holds a document's height and the floor at or
    /// below which a replicated document is refused:
    /// `{"<collectionID>": {"field": "blockNumber", "floor": 100}}`.
    pub retention_json: *const c_char,
    /// Optional replication filter, asked whether to keep each replicated
    /// document before its commit stores anything; a
    /// `DefraReplicationFilterCallback`, spelled out so C sees a nullable
    /// function pointer. Null keeps everything.
    pub replication_filter: Option<
        unsafe extern "C" fn(
            context: usize,
            collection_id: *const c_char,
            doc_id: *const c_char,
            fields_json: *const c_char,
        ) -> c_int,
    >,
    /// Passed back to `replication_filter` on every call.
    pub replication_filter_context: usize,
    /// Optional retention rule asked as documents arrive (Go Shinzo's
    /// `client.RetentionRule`); it replaces `retention_json`. Called with
    /// `retention_rule_context` and a collection ID, it returns non-zero when
    /// the rule covers the collection, writing the height field's name,
    /// null-terminated, into `field_out` (`field_capacity` bytes) and the
    /// floor into `floor_out`. Thread-safe and non-blocking, as
    /// `replication_filter`.
    pub retention_rule: Option<
        unsafe extern "C" fn(
            context: usize,
            collection_id: *const c_char,
            field_out: *mut c_char,
            field_capacity: usize,
            floor_out: *mut i64,
        ) -> c_int,
    >,
    /// Passed back to `retention_rule` on every call.
    pub retention_rule_context: usize,
    /// Optional Ed25519 peer key, 64 bytes (seed then public key, as Go's
    /// libp2p `PrivKey.Raw()` gives it): the node's peer ID on every start.
    /// A store that already holds a different, stored peer key refuses it.
    pub p2p_private_key: *const u8,
    /// Length of `p2p_private_key`.
    pub p2p_private_key_len: usize,
    /// Optional JSON array of libp2p listen multiaddrs; when set it replaces
    /// `new_node_with_p2p`'s single address.
    pub p2p_listen_addrs_json: *const c_char,
    /// Optional JSON array of peers the libp2p host connects to at start.
    pub p2p_bootstrap_peers_json: *const c_char,
    /// Non-zero runs libp2p without GossipSub.
    pub p2p_disable_pubsub: c_int,
    /// Non-zero runs libp2p without the relay client transport.
    pub p2p_disable_relay: c_int,
    /// libp2p resource manager memory budget in MiB; zero keeps the default.
    pub p2p_resource_memory_mib: u64,
    /// libp2p resource manager file-descriptor budget; zero keeps the default.
    pub p2p_resource_file_descriptors: u64,
    /// Streams one peer may hold open; zero keeps the default.
    pub p2p_max_streams_per_peer: usize,
}

impl Default for NodeInitOptions {
    fn default() -> Self {
        Self {
            db_path: ptr::null(),
            in_memory: 1, // Default to in-memory
            datastore_backend: ptr::null(),
            enable_signing: 0,
            signing_key_type: ptr::null(),
            signing_private_key: ptr::null(),
            signing_private_key_len: 0,
            vera_grpc_address: ptr::null(),
            vera_comet_rpc_address: ptr::null(),
            vera_chain_id: ptr::null(),
            vera_signer_key: ptr::null(),
            vera_signer_key_len: 0,
            p2p_transport: ptr::null(),
            iroh_relay_url: ptr::null(),
            iroh_relay_mode: ptr::null(),
            iroh_relay_urls_json: ptr::null(),
            iroh_bind_addr: ptr::null(),
            iroh_bind_port: 0,
            iroh_discovery: 1,
            iroh_discovery_origin_domain: ptr::null(),
            iroh_pkarr_relay_url: ptr::null(),
            iroh_key_path: ptr::null(),
            max_concurrent_dag_fetches: 0,
            max_concurrent_push_tasks: 0,
            max_doc_sync_request_doc_ids: 0,
            rate_limit_burst: 0,
            rate_limit_rate: 0.0,
            shinzo_wire: 0,
            retention_json: ptr::null(),
            replication_filter: None,
            replication_filter_context: 0,
            retention_rule: None,
            retention_rule_context: 0,
            p2p_private_key: ptr::null(),
            p2p_private_key_len: 0,
            p2p_listen_addrs_json: ptr::null(),
            p2p_bootstrap_peers_json: ptr::null(),
            p2p_disable_pubsub: 0,
            p2p_disable_relay: 0,
            p2p_resource_memory_mib: 0,
            p2p_resource_file_descriptors: 0,
            p2p_max_streams_per_peer: 0,
        }
    }
}

/// Trait for FFI return types that can represent a panic error.
///
/// Used by the `ffi_entry!` macro to convert caught panics into
/// error results instead of unwinding across the FFI boundary.
pub trait FfiPanicResult {
    fn from_panic(msg: String) -> Self;
}

impl FfiPanicResult for FfiResult {
    fn from_panic(msg: String) -> Self {
        FfiResult::error(msg)
    }
}

impl FfiPanicResult for NewNodeResult {
    fn from_panic(msg: String) -> Self {
        NewNodeResult::error(msg)
    }
}

impl FfiPanicResult for NewTxnResult {
    fn from_panic(msg: String) -> Self {
        NewTxnResult::error(msg)
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

/// Free a string allocated by FFI functions.
///
/// # Safety
///
/// The pointer must have been allocated by an FFI function in this crate.
#[no_mangle]
pub unsafe extern "C" fn defra_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_result_success() {
        let result = FfiResult::success(r#"{"data": "test"}"#);
        assert_eq!(result.status, 0);
        assert!(result.error.is_null());
        assert!(!result.value.is_null());

        // Clean up
        unsafe { defra_free_string(result.value) };
    }

    #[test]
    fn test_ffi_result_error() {
        let result = FfiResult::error("something went wrong");
        assert_eq!(result.status, 1);
        assert!(!result.error.is_null());
        assert!(result.value.is_null());

        // Clean up
        unsafe { defra_free_string(result.error) };
    }

    #[test]
    fn test_new_node_result() {
        let result = NewNodeResult::success(42);
        assert_eq!(result.status, 0);
        assert_eq!(result.node_ptr, 42);
        assert!(result.error.is_null());
    }

    // Edge case tests for null byte handling (H2)

    #[test]
    fn test_ffi_result_success_with_null_bytes() {
        // String with embedded null byte should not panic
        let value_with_null = "hello\0world";
        let result = FfiResult::success(value_with_null);
        assert_eq!(result.status, 0);
        assert!(!result.value.is_null());

        // Value should have null bytes replaced
        let value = unsafe { CStr::from_ptr(result.value).to_string_lossy() };
        assert!(value.contains('\u{FFFD}'), "null byte should be replaced");
        assert!(!value.contains('\0'), "should not contain null byte");

        unsafe { defra_free_string(result.value) };
    }

    #[test]
    fn test_ffi_result_error_with_null_bytes() {
        // Error message with embedded null byte should not panic
        let error_with_null = "error\0message";
        let result = FfiResult::error(error_with_null);
        assert_eq!(result.status, 1);
        assert!(!result.error.is_null());

        // Error should have null bytes replaced
        let error = unsafe { CStr::from_ptr(result.error).to_string_lossy() };
        assert!(error.contains('\u{FFFD}'), "null byte should be replaced");
        assert!(!error.contains('\0'), "should not contain null byte");

        unsafe { defra_free_string(result.error) };
    }

    #[test]
    fn test_new_node_result_error_with_null_bytes() {
        // Error message with embedded null byte should not panic
        let error_with_null = "node\0error";
        let result = NewNodeResult::error(error_with_null);
        assert_eq!(result.status, 1);
        assert!(!result.error.is_null());
        assert_eq!(result.node_ptr, 0);

        // Error should have null bytes replaced
        let error = unsafe { CStr::from_ptr(result.error).to_string_lossy() };
        assert!(error.contains('\u{FFFD}'), "null byte should be replaced");

        unsafe { defra_free_string(result.error) };
    }

    #[test]
    fn test_c_str_to_string_null_ptr() {
        let result = unsafe { c_str_to_string(ptr::null()) };
        assert!(result.is_none());
    }

    #[test]
    fn test_ffi_result_ok() {
        let result = FfiResult::ok();
        assert_eq!(result.status, 0);
        assert!(result.error.is_null());
        assert!(result.value.is_null());
    }
}
