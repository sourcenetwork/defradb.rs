//! Schema operations for FFI.
//!
//! This module exposes schema management functions that match
//! Go's cbindings/schema.go behavior.

use std::ffi::c_char;
use std::sync::Arc;

use acp::nac::NodePermission;
use identity::Did;

use crate::ffi_entry;
use crate::helpers::{get_node_database, require_c_str};
use crate::nac_check::check_nac_for_node;
use crate::policy_yaml;
use crate::state::{PolicyStore, NODES};
use crate::types::FfiResult;
use crate::{ffi_async, try_ffi, ERR_INVALID_NODE_HANDLE};

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub(crate) fn parse_optional_identity_did(
    identity_did: *const c_char,
) -> Result<(Option<String>, Option<Did>), FfiResult> {
    // SAFETY: `identity_did` is either null or a valid C string from the FFI caller.
    let identity_str =
        unsafe { crate::types::c_str_to_string(identity_did) }.filter(|s| !s.is_empty());
    let creator = match identity_str.as_deref() {
        Some(did) => Some(
            Did::new(did).map_err(|e| FfiResult::error(format!("invalid identity DID: {}", e)))?,
        ),
        None => None,
    };
    Ok((identity_str, creator))
}

/// Add a schema to the database.
///
/// The schema should be a GraphQL SDL string defining types.
///
/// Returns a JSON array of CollectionVersion objects on success.
///
/// # Example SDL
///
/// ```graphql
/// type User {
///     name: String
///     age: Int
/// }
/// ```
///
/// # Safety
///
/// `schema_sdl` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn add_schema(
    node_ptr: usize,
    identity_did: *const c_char,
    schema_sdl: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let schema_str = try_ffi!(require_c_str(schema_sdl, "schema_sdl"));

        let database = try_ffi!(get_node_database(node_ptr));
        let (identity_str, _) = try_ffi!(parse_optional_identity_did(identity_did));

        // The caller acts in the call's context, so the NAC checks the core
        // makes resolve the actual caller instead of the wildcard.
        let context = defra_core::RequestContext::default().with_identity(identity_str);

        ffi_async!(node_ptr, {
            let created_versions = client::Client::add_collection(&*database, &context, &schema_str)
                .await
                .map_err(|e| e.to_string())?;

            // Return JSON array of created collection versions
            let json = serde_json::to_string(&created_versions)
                .map_err(|e| format!("failed to serialize result: {}", e))?;

            Ok(json)
        })
    }
}

/// Add a schema within a specific transaction.
///
/// The schema is only visible within the transaction until it is committed.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid, non-null, and point to valid C strings.
#[no_mangle]
pub unsafe extern "C" fn add_schema_in_txn(
    node_ptr: usize,
    txn_id: *const c_char,
    identity_did: *const c_char,
    schema_sdl: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let txn_str = try_ffi!(require_c_str(txn_id, "txn_id"));
        let schema_str = try_ffi!(require_c_str(schema_sdl, "schema_sdl"));

        let registry = match NODES.get(node_ptr, |state| state.txn_registry.clone()) {
            Some(registry) => registry,
            None => return FfiResult::error(ERR_INVALID_NODE_HANDLE),
        };
        let (identity_str, _) = try_ffi!(parse_optional_identity_did(identity_did));

        // The caller acts in the call's context, so the NAC checks the core
        // makes resolve the actual caller instead of the wildcard.
        let context = defra_core::RequestContext::default().with_identity(identity_str);

        ffi_async!(caller, {
            let created_versions = registry
                .add_collection_in_txn_as(&context, &txn_str, &schema_str)
                .await
                .map_err(|e| e.to_string())?;

            let json = serde_json::to_string(&created_versions)
                .map_err(|e| format!("failed to serialize result: {}", e))?;

            Ok(json)
        })
    }
}

/// Get all collections from the database.
///
/// Returns a JSON array of collection descriptions.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid, non-null, and point to valid C strings.
#[no_mangle]
pub unsafe extern "C" fn get_collections(
    node_ptr: usize,
    identity_did: *const c_char,
) -> FfiResult {
    ffi_entry! {
        try_ffi!(check_nac_for_node(
            node_ptr,
            identity_did,
            NodePermission::CollectionGet
        ));
        let database = try_ffi!(get_node_database(node_ptr));

        ffi_async!(node_ptr, {
            // Return all collection versions from the system store (active + inactive + placeholders).
            // The Go wrapper handles GetInactive filtering on its side.
            let collections = database
                .get_all_collection_versions()
                .await
                .map_err(|e| format!("failed to get collections: {}", e))?;

            // Return JSON array
            let json = serde_json::to_string(&collections)
                .map_err(|e| format!("failed to serialize result: {}", e))?;

            Ok(json)
        })
    }
}

/// Get all collection versions visible within a specific transaction.
///
/// This reads from the transaction's systemstore, which includes uncommitted
/// writes (e.g., placeholders from set_migration_in_txn).
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid, non-null, and point to valid C strings.
#[no_mangle]
pub unsafe extern "C" fn get_collections_in_txn(
    node_ptr: usize,
    txn_id: *const c_char,
    identity_did: *const c_char,
) -> FfiResult {
    ffi_entry! {
        try_ffi!(check_nac_for_node(
            node_ptr,
            identity_did,
            NodePermission::CollectionGet
        ));
        let txn_str = try_ffi!(require_c_str(txn_id, "txn_id"));

        let registry = match NODES.get(node_ptr, |state| state.txn_registry.clone()) {
            Some(r) => r,
            None => return FfiResult::error(ERR_INVALID_NODE_HANDLE),
        };

        // The caller acts in the call's context, so the NAC checks the body
        // reaches resolve the actual caller instead of the wildcard.
        let context = defra_core::RequestContext::default().with_identity(crate::types::c_str_to_string(identity_did).filter(|s| !s.is_empty()));

        ffi_async!(caller, {
            let collections = registry
                .get_collections_in_txn_as(&context, &txn_str)
                .await
                .map_err(|e| format!("failed to get collections in txn: {}", e))?;

            let json = serde_json::to_string(&collections)
                .map_err(|e| format!("failed to serialize result: {}", e))?;

            Ok(json)
        })
    }
}

/// Validate that a collection's policy references a valid, well-formed policy.
pub(crate) fn validate_collection_policy(
    policy: &schema::PolicyDescription,
    store: &Arc<PolicyStore>,
) -> Result<(), String> {
    // 1. Check policy exists in the store
    let policy_yaml = store
        .get_policy(&policy.id)
        .ok_or("policyID specified does not exist with acp")?;

    // 2. Parse the YAML to inspect structure
    let parsed = policy_yaml::parse_policy_yaml(&policy_yaml)
        .map_err(|e| format!("failed to parse policy: {}", e))?;

    // 3. Check the referenced resource exists
    let resource = parsed
        .find_resource(&policy.resource_name)
        .ok_or("resource does not exist on the specified policy")?;

    // 4. Check required permissions (read, update, delete)
    for required in &["read", "update", "delete"] {
        if !resource.has_permission(required) {
            return Err("resource is missing required permission on policy.".to_string());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{new_node, node_close};
    use crate::types::NodeInitOptions;
    use std::ffi::CString;

    #[test]
    fn test_add_schema_policy_must_exist() {
        assert!(crate::runtime::init_runtime());
        let result = new_node(NodeInitOptions::default());
        assert_eq!(result.status, 0);
        let node = result.node_ptr;
        let owner =
            CString::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap();

        let policy = CString::new(
            "name: P\nresources:\n  - name: users\n    relations:\n      - name: viewer\n    permissions:\n      - name: read\n        expr: viewer\n      - name: update\n        expr: viewer\n      - name: delete\n        expr: viewer\n",
        )
        .unwrap();
        let added = unsafe { crate::acp::add_dac_policy(node, owner.as_ptr(), policy.as_ptr()) };
        assert_eq!(added.status, 0, "add_dac_policy should succeed");
        let json = unsafe { std::ffi::CStr::from_ptr(added.value) }
            .to_str()
            .unwrap()
            .to_string();
        let id = serde_json::from_str::<serde_json::Value>(&json).unwrap()["PolicyID"]
            .as_str()
            .unwrap()
            .to_string();

        let known = CString::new(format!(
            "type Known @policy(id: \"{id}\", resource: \"users\") {{ name: String }}"
        ))
        .unwrap();
        let ok = unsafe { add_schema(node, owner.as_ptr(), known.as_ptr()) };
        assert_eq!(ok.status, 0, "a policy added through FFI must be found");

        let missing = CString::new(
            "type Lost @policy(id: \"deadbeef\", resource: \"users\") { name: String }",
        )
        .unwrap();
        let bad = unsafe { add_schema(node, owner.as_ptr(), missing.as_ptr()) };
        assert_ne!(bad.status, 0, "a missing policy must be rejected");

        assert_eq!(node_close(node).status, 0);
    }

    #[test]
    fn test_add_schema_and_get_collections() {
        // Initialize runtime
        assert!(crate::runtime::init_runtime());

        // Create node
        let options = NodeInitOptions::default();
        let result = new_node(options);
        assert_eq!(result.status, 0);
        let node = result.node_ptr;

        // Add schema
        let sdl = CString::new("type User { name: String }").unwrap();
        let result = unsafe { add_schema(node, std::ptr::null(), sdl.as_ptr()) };
        assert_eq!(result.status, 0, "add_schema should succeed");

        // Get collections
        let result = unsafe { get_collections(node, std::ptr::null()) };
        assert_eq!(result.status, 0, "get_collections should succeed");
        assert!(!result.value.is_null());

        // Check value contains User
        let value = unsafe { std::ffi::CStr::from_ptr(result.value).to_string_lossy() };
        assert!(value.contains("User"), "should contain User collection");

        // Cleanup
        unsafe {
            crate::types::defra_free_string(result.value);
        }
        node_close(node);
    }

    #[test]
    fn test_add_schema_duplicate_implicit_relation_name_errors() {
        assert!(crate::runtime::init_runtime());

        let options = NodeInitOptions::default();
        let result = new_node(options);
        assert_eq!(result.status, 0);
        let node = result.node_ptr;

        let sdl = CString::new(
            r#"
            type Book {
                title: String
                author: Person
                reviewer: Person
            }

            type Person {
                name: String
                authoredBooks: [Book]
                reviewedBooks: [Book]
            }
            "#,
        )
        .unwrap();
        let result = unsafe { add_schema(node, std::ptr::null(), sdl.as_ptr()) };
        assert_eq!(result.status, 1);
        let error = unsafe { std::ffi::CStr::from_ptr(result.error).to_string_lossy() };
        assert!(
            error.contains(
                "relation name is not unique within collection. Field: author, RelationName: book_person"
            ),
            "unexpected error: {error}"
        );
        unsafe {
            crate::types::defra_free_string(result.error);
        }

        node_close(node);
    }
}
