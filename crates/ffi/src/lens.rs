//! Lens FFI functions: Go's `AddLens` and `ListLenses`, each a call to the
//! core through the node's executor, in the transaction a `*_in_txn`
//! variant names.

use std::ffi::c_char;

use crate::ffi_entry;
use crate::helpers::require_c_str;
use crate::types::FfiResult;
use crate::{ffi_async, try_ffi};

/// The context `identity_did` acts in, in transaction `txn_id` when it is set.
///
/// # Safety
///
/// Both pointers must be null or valid null-terminated UTF-8 strings.
pub(crate) unsafe fn call_context(
    identity_did: *const c_char,
    txn_id: *const c_char,
) -> Result<defra_core::RequestContext, FfiResult> {
    let identity = crate::types::c_str_to_string(identity_did).filter(|s| !s.is_empty());
    let txn = match txn_id.is_null() {
        true => None,
        false => Some(
            require_c_str(txn_id, "txn_id")?
                .parse::<defra_core::TxnId>()
                .map_err(|e| FfiResult::error(format!("invalid transaction ID: {e}")))?,
        ),
    };
    Ok(defra_core::RequestContext::default()
        .with_identity(identity)
        .with_txn(txn))
}

/// Store a lens, Go's `model.Lens` JSON (`{"Lenses": [...]}`); the result is
/// its content ID.
unsafe fn add(
    node_ptr: usize,
    txn_id: *const c_char,
    identity_did: *const c_char,
    lens_json: *const c_char,
) -> FfiResult {
    let context = try_ffi!(call_context(identity_did, txn_id));
    let lens_str = try_ffi!(require_c_str(lens_json, "lens_json"));
    let runner = try_ffi!(crate::helpers::get_node_runner(node_ptr));
    ffi_async!(caller, {
        let config: lens::LensConfig = serde_json::from_str(&lens_str)
            .map_err(|e| format!("failed to parse lens config: {e}"))?;
        runner
            .add_lens_as(&context, config)
            .await
            .map_err(|e| e.to_string())
    })
}

/// Every stored lens, a JSON object from content ID to Go's `model.Lens`.
unsafe fn list(node_ptr: usize, txn_id: *const c_char, identity_did: *const c_char) -> FfiResult {
    let context = try_ffi!(call_context(identity_did, txn_id));
    let runner = try_ffi!(crate::helpers::get_node_runner(node_ptr));
    ffi_async!(caller, {
        runner
            .list_lenses_as(&context)
            .await
            .map_err(|e| e.to_string())
    })
}

/// Store a lens. See [`add`].
///
/// # Safety
///
/// `identity_did` and `lens_json` must be null or valid null-terminated
/// UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn lens_add(
    node_ptr: usize,
    identity_did: *const c_char,
    lens_json: *const c_char,
) -> FfiResult {
    ffi_entry! { add(node_ptr, std::ptr::null(), identity_did, lens_json) }
}

/// Store a lens in transaction `txn_id`. See [`add`].
///
/// # Safety
///
/// Every string pointer must be null or a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn lens_add_in_txn(
    node_ptr: usize,
    txn_id: *const c_char,
    identity_did: *const c_char,
    lens_json: *const c_char,
) -> FfiResult {
    ffi_entry! { add(node_ptr, txn_id, identity_did, lens_json) }
}

/// Every stored lens. See [`list`].
///
/// # Safety
///
/// `identity_did` must be null or a valid null-terminated UTF-8 string. The
/// caller frees the returned string with `defra_free_string`.
#[no_mangle]
pub unsafe extern "C" fn lens_list(node_ptr: usize, identity_did: *const c_char) -> FfiResult {
    ffi_entry! { list(node_ptr, std::ptr::null(), identity_did) }
}

/// Every lens transaction `txn_id` sees. See [`list`].
///
/// # Safety
///
/// Both string pointers must be null or valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn lens_list_in_txn(
    node_ptr: usize,
    txn_id: *const c_char,
    identity_did: *const c_char,
) -> FfiResult {
    ffi_entry! { list(node_ptr, txn_id, identity_did) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{new_node, node_close};
    use crate::types::NodeInitOptions;
    use std::ffi::CString;

    #[test]
    fn test_lens_add_invalid_node() {
        assert!(crate::runtime::init_runtime());

        let lens_json = CString::new(r#"{"Path": "/path/to/transform.wasm"}"#).unwrap();
        let result = unsafe { lens_add(0, std::ptr::null(), lens_json.as_ptr()) };
        assert_eq!(result.status, 1);
        assert!(!result.error.is_null());
        unsafe { crate::types::defra_free_string(result.error) };
    }

    #[test]
    fn test_lens_add_null_json() {
        assert!(crate::runtime::init_runtime());

        let options = NodeInitOptions::default();
        let result = new_node(options);
        assert_eq!(result.status, 0);
        let node = result.node_ptr;

        let result = unsafe { lens_add(node, std::ptr::null(), std::ptr::null()) };
        assert_eq!(result.status, 1);
        assert!(!result.error.is_null());
        unsafe { crate::types::defra_free_string(result.error) };

        node_close(node);
    }

    #[test]
    fn test_lens_list_empty() {
        assert!(crate::runtime::init_runtime());

        let options = NodeInitOptions::default();
        let result = new_node(options);
        assert_eq!(result.status, 0);
        let node = result.node_ptr;

        let result = unsafe { lens_list(node, std::ptr::null()) };
        assert_eq!(result.status, 0);
        assert!(!result.value.is_null());

        let value = unsafe { std::ffi::CStr::from_ptr(result.value).to_string_lossy() };
        // Should be empty object
        assert!(value == "{}" || value == "[]", "should be empty: {}", value);
        unsafe { crate::types::defra_free_string(result.value) };

        node_close(node);
    }

    #[test]
    fn test_lens_list_invalid_node() {
        assert!(crate::runtime::init_runtime());

        let result = unsafe { lens_list(0, std::ptr::null()) };
        assert_eq!(result.status, 1);
        assert!(!result.error.is_null());
        unsafe { crate::types::defra_free_string(result.error) };
    }
}
