use std::ffi::c_char;

use crate::helpers::{get_node_runner, require_c_str};
use crate::state::NODES;
use crate::types::{c_str_to_string, FfiResult};
use crate::{ffi_async, ffi_async_ok, ffi_entry, try_ffi, ERR_INVALID_NODE_HANDLE};

/// Get the current NAC status, Go's `GetNACStatus`; the core checks
/// `NacStatus` for `identity_did`.
///
/// Returns a JSON object with NAC status information:
/// ```json
/// {
///   "status": "enabled" | "disabled temporarily" | "not configured",
///   "configured_enabled": true | false,
///   "dev_mode": true | false,
///   "owner": "did:key:..." | null
/// }
/// ```
///
/// # Safety
///
/// `identity_did` must be null or a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn get_nac_status(node_ptr: usize, identity_did: *const c_char) -> FfiResult {
    ffi_entry! {
        let context = try_ffi!(crate::lens::call_context(identity_did, std::ptr::null()));
        let runner = try_ffi!(get_node_runner(node_ptr));
        let nac_manager = match NODES.get(node_ptr, |state| state.nac_manager.clone()) {
            Some(m) => m,
            None => return FfiResult::error(ERR_INVALID_NODE_HANDLE),
        };

        ffi_async!(node_ptr, {
            let status = runner.nac_status_as(&context).await.map_err(|e| e.to_string())?;
            let mut info = nac_manager.info().await;
            info.status = status.to_string();
            serde_json::to_string(&info).map_err(|e| format!("failed to serialize NAC info: {}", e))
        })
    }
}

/// Temporarily disable NAC, Go's `DisableNAC`, as `requestor_did`.
///
/// # Safety
///
/// `requestor_did` must be null or a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn disable_nac(node_ptr: usize, requestor_did: *const c_char) -> FfiResult {
    ffi_entry! {
        let context = try_ffi!(crate::lens::call_context(requestor_did, std::ptr::null()));
        let runner = try_ffi!(get_node_runner(node_ptr));
        ffi_async_ok!(node_ptr, {
            runner.disable_nac_as(&context).await.map_err(|e| e.to_string())
        })
    }
}

/// Re-enable a disabled NAC, Go's `ReEnableNAC`, as `requestor_did`.
///
/// # Safety
///
/// `requestor_did` must be null or a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn re_enable_nac(node_ptr: usize, requestor_did: *const c_char) -> FfiResult {
    ffi_entry! {
        let context = try_ffi!(crate::lens::call_context(requestor_did, std::ptr::null()));
        let runner = try_ffi!(get_node_runner(node_ptr));
        ffi_async_ok!(node_ptr, {
            runner.re_enable_nac_as(&context).await.map_err(|e| e.to_string())
        })
    }
}

/// Enable NAC with the given owner identity.
///
/// This initializes NAC and sets the owner. Can only be called when NAC
/// is not already enabled.
///
/// # Safety
///
/// `owner_did` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn enable_nac(node_ptr: usize, owner_did: *const c_char) -> FfiResult {
    ffi_entry! {
        let owner_str = try_ffi!(require_c_str(owner_did, "owner_did"));

        let nac_manager = match NODES.get(node_ptr, |state| state.nac_manager.clone()) {
            Some(m) => m,
            None => return FfiResult::error(ERR_INVALID_NODE_HANDLE),
        };

        ffi_async_ok!(node_ptr, {
            let owner = identity::Did::new(&owner_str)
                .map_err(|e| format!("invalid DID '{}': {}", owner_str, e))?;

            nac_manager
                .enable(&owner)
                .await
                .map_err(|e| format!("failed to enable NAC: {}", e))?;

            Ok(())
        })
    }
}

/// The relation a relationship call names; `admin` when it names none.
unsafe fn relation_or_admin(relation: *const c_char) -> String {
    match c_str_to_string(relation) {
        Some(relation) if !relation.is_empty() => relation,
        _ => "admin".to_string(),
    }
}

/// Add a NAC actor relationship, Go's `AddNACActorRelationship`, as
/// `requestor_did`; the core checks the caller and the relation.
///
/// Returns JSON with success status:
/// ```json
/// { "added": true }  // or false if already exists
/// ```
///
/// # Safety
///
/// All string parameters must be null or valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn add_nac_actor_relationship(
    node_ptr: usize,
    requestor_did: *const c_char,
    relation: *const c_char,
    target_did: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let context = try_ffi!(crate::lens::call_context(requestor_did, std::ptr::null()));
        let relation = relation_or_admin(relation);
        let target = c_str_to_string(target_did).unwrap_or_default();
        let runner = try_ffi!(get_node_runner(node_ptr));
        ffi_async!(node_ptr, {
            let existed = runner
                .add_nac_actor_relationship_as(&context, &relation, &target)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "added": !existed }).to_string())
        })
    }
}

/// Delete a NAC actor relationship, Go's `DeleteNACActorRelationship`, as
/// `requestor_did`; the core checks the caller and the relation.
///
/// Returns JSON with success status:
/// ```json
/// { "deleted": true }  // or false if didn't exist
/// ```
///
/// # Safety
///
/// All string parameters must be null or valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn delete_nac_actor_relationship(
    node_ptr: usize,
    requestor_did: *const c_char,
    relation: *const c_char,
    target_did: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let context = try_ffi!(crate::lens::call_context(requestor_did, std::ptr::null()));
        let relation = relation_or_admin(relation);
        let target = c_str_to_string(target_did).unwrap_or_default();
        let runner = try_ffi!(get_node_runner(node_ptr));
        ffi_async!(node_ptr, {
            let deleted = runner
                .delete_nac_actor_relationship_as(&context, &relation, &target)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "deleted": deleted }).to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{new_node, node_close};
    use crate::types::NodeInitOptions;
    use std::ffi::{CStr, CString};

    fn test_did() -> &'static str {
        "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"
    }

    fn test_did2() -> &'static str {
        "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH"
    }

    #[test]
    fn test_get_nac_status() {
        assert!(crate::runtime::init_runtime());

        let options = NodeInitOptions::default();
        let result = new_node(options);
        assert_eq!(result.status, 0);
        let node = result.node_ptr;

        // Get NAC status
        let result = unsafe { get_nac_status(node, std::ptr::null()) };
        assert_eq!(result.status, 0, "get_nac_status should succeed");
        assert!(!result.value.is_null());

        let value = unsafe { CStr::from_ptr(result.value).to_string_lossy() };
        assert!(
            value.contains("not configured"),
            "NAC should not be configured initially"
        );

        unsafe { crate::types::defra_free_string(result.value) };
        node_close(node);
    }

    #[test]
    fn test_enable_nac() {
        assert!(crate::runtime::init_runtime());

        let options = NodeInitOptions::default();
        let result = new_node(options);
        assert_eq!(result.status, 0);
        let node = result.node_ptr;

        // Enable NAC
        let owner_did = CString::new(test_did()).unwrap();
        let result = unsafe { enable_nac(node, owner_did.as_ptr()) };
        assert_eq!(result.status, 0, "enable_nac should succeed");

        // Verify NAC is now enabled (pass owner DID since NAC is active)
        let result = unsafe { get_nac_status(node, owner_did.as_ptr()) };
        assert_eq!(result.status, 0);
        let value = unsafe { CStr::from_ptr(result.value).to_string_lossy() };
        assert!(value.contains("enabled"), "NAC should be enabled");
        assert!(value.contains(test_did()), "should show owner DID");

        unsafe { crate::types::defra_free_string(result.value) };
        node_close(node);
    }

    #[test]
    fn test_disable_and_re_enable_nac() {
        assert!(crate::runtime::init_runtime());

        let options = NodeInitOptions::default();
        let result = new_node(options);
        assert_eq!(result.status, 0);
        let node = result.node_ptr;

        // Enable NAC first
        let owner_did = CString::new(test_did()).unwrap();
        let result = unsafe { enable_nac(node, owner_did.as_ptr()) };
        assert_eq!(result.status, 0);

        // Disable NAC
        let result = unsafe { disable_nac(node, owner_did.as_ptr()) };
        assert_eq!(result.status, 0, "disable_nac should succeed");

        // Verify NAC is disabled (null identity OK since NAC is disabled)
        let result = unsafe { get_nac_status(node, std::ptr::null()) };
        assert_eq!(
            result.status, 0,
            "get_nac_status should succeed when disabled"
        );
        let value = unsafe { CStr::from_ptr(result.value).to_string_lossy() };
        assert!(
            value.contains("disabled temporarily"),
            "NAC should be disabled, got: {}",
            value
        );
        unsafe { crate::types::defra_free_string(result.value) };

        // Re-enable NAC
        let result = unsafe { re_enable_nac(node, owner_did.as_ptr()) };
        assert_eq!(result.status, 0, "re_enable_nac should succeed");

        // Verify NAC is enabled again (pass owner DID since NAC is active)
        let result = unsafe { get_nac_status(node, owner_did.as_ptr()) };
        let value = unsafe { CStr::from_ptr(result.value).to_string_lossy() };
        assert!(
            value.contains("\"status\":\"enabled\""),
            "NAC should be re-enabled"
        );
        unsafe { crate::types::defra_free_string(result.value) };

        node_close(node);
    }

    #[test]
    fn test_re_enable_nac_checks_authorization_before_enabled_state() {
        assert!(crate::runtime::init_runtime());

        let result = new_node(NodeInitOptions::default());
        assert_eq!(result.status, 0);
        let node = result.node_ptr;

        let owner_did = CString::new(test_did()).unwrap();
        let result = unsafe { enable_nac(node, owner_did.as_ptr()) };
        assert_eq!(result.status, 0);

        for requestor in ["", test_did2()] {
            let requestor = CString::new(requestor).unwrap();
            let result = unsafe { re_enable_nac(node, requestor.as_ptr()) };
            assert_eq!(result.status, 1);
            let error = unsafe { CStr::from_ptr(result.error).to_string_lossy() };
            assert_eq!(
                error,
                "not authorized to perform operation. Permission: re-enable-nac"
            );
            unsafe { crate::types::defra_free_string(result.error) };
        }

        let result = unsafe { re_enable_nac(node, owner_did.as_ptr()) };
        assert_eq!(result.status, 1);
        let error = unsafe { CStr::from_ptr(result.error).to_string_lossy() };
        assert_eq!(error, "node acp is already enabled");
        unsafe { crate::types::defra_free_string(result.error) };

        node_close(node);
    }

    #[test]
    fn test_nac_actor_relationship() {
        assert!(crate::runtime::init_runtime());

        let options = NodeInitOptions::default();
        let result = new_node(options);
        assert_eq!(result.status, 0);
        let node = result.node_ptr;

        // Enable NAC first
        let owner_did = CString::new(test_did()).unwrap();
        let result = unsafe { enable_nac(node, owner_did.as_ptr()) };
        assert_eq!(result.status, 0);

        // Add admin relationship
        let target_did = CString::new(test_did2()).unwrap();
        let relation = CString::new("admin").unwrap();
        let result = unsafe {
            add_nac_actor_relationship(
                node,
                owner_did.as_ptr(),
                relation.as_ptr(),
                target_did.as_ptr(),
            )
        };
        assert_eq!(
            result.status, 0,
            "add_nac_actor_relationship should succeed"
        );
        let value = unsafe { CStr::from_ptr(result.value).to_string_lossy() };
        assert!(value.contains("\"added\":true"), "should indicate added");
        unsafe { crate::types::defra_free_string(result.value) };

        // Delete admin relationship
        let result = unsafe {
            delete_nac_actor_relationship(
                node,
                owner_did.as_ptr(),
                relation.as_ptr(),
                target_did.as_ptr(),
            )
        };
        assert_eq!(
            result.status, 0,
            "delete_nac_actor_relationship should succeed"
        );
        let value = unsafe { CStr::from_ptr(result.value).to_string_lossy() };
        assert!(
            value.contains("\"deleted\":true"),
            "should indicate deleted"
        );
        unsafe { crate::types::defra_free_string(result.value) };

        node_close(node);
    }

    #[test]
    fn test_invalid_did_handling() {
        assert!(crate::runtime::init_runtime());

        let options = NodeInitOptions::default();
        let result = new_node(options);
        assert_eq!(result.status, 0);
        let node = result.node_ptr;

        // Try to enable NAC with invalid DID
        let invalid_did = CString::new("invalid-did").unwrap();
        let result = unsafe { enable_nac(node, invalid_did.as_ptr()) };
        assert_eq!(result.status, 1, "should fail with invalid DID");
        let error = unsafe { CStr::from_ptr(result.error).to_string_lossy() };
        assert!(error.contains("invalid DID"), "should indicate invalid DID");

        unsafe { crate::types::defra_free_string(result.error) };
        node_close(node);
    }

    #[test]
    fn test_null_pointer_handling() {
        assert!(crate::runtime::init_runtime());

        let options = NodeInitOptions::default();
        let result = new_node(options);
        assert_eq!(result.status, 0);
        let node = result.node_ptr;

        // Test null pointer handling
        let result = unsafe { enable_nac(node, std::ptr::null()) };
        assert_eq!(result.status, 1, "should fail with null pointer");
        let error = unsafe { CStr::from_ptr(result.error).to_string_lossy() };
        assert!(error.contains("null"), "should indicate null pointer");

        unsafe { crate::types::defra_free_string(result.error) };
        node_close(node);
    }
}
