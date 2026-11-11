//! FFI type definitions matching Go's cbindings/defra_structs.h

use std::ffi::{c_char, c_int, CStr, CString};
use std::ptr;

mod node_init_options;

pub use node_init_options::NodeInitOptions;

/// FFI result type matching Go's Result struct.
///
/// Status codes:
/// - 0: Success
/// - 1: Error (message in error field)
/// - 2: Subscription: `defra_query_execute` started a GraphQL subscription,
///   whose ID is in the value field; read its events with
///   `defra_subscription_poll` and end it with `defra_subscription_close`
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
mod tests;
