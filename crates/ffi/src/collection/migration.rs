use std::ffi::c_char;

use acp::nac::NodePermission;

use crate::ffi_node_db_async_body;
use crate::helpers::{get_node_database, require_c_str};
use crate::nac_check::check_nac_for_node;
use crate::types::FfiResult;
use crate::{ffi_async, ffi_entry, try_ffi};

/// Eagerly migrate and cache every known-version document in a collection.
///
/// This advances datastore values, stored document-version keys, and secondary
/// indexes without creating or broadcasting document commits.
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
/// * `identity_did` - Optional DID for permission checks (null for anonymous)
/// * `collection_name` - Name of the collection to materialize
///
/// # Returns
///
/// - Status 0: Success (value contains the number of documents advanced)
/// - Status 1: Error (error field contains message)
///
/// # Safety
///
/// `identity_did` and `collection_name` must be valid null-terminated UTF-8
/// strings when non-null.
#[no_mangle]
pub unsafe extern "C" fn materialize_collection(
    node_ptr: usize,
    identity_did: *const c_char,
    collection_name: *const c_char,
) -> FfiResult {
    ffi_node_db_async_body! {
        node = node_ptr,
        identity = identity_did,
        database = database,
        context = context,
        permission = NodePermission::CollectionPatch,
        collection_name => collection_name_str: "collection_name";
        {
            let count = database
                .materialize_collection_as(&context, &collection_name_str)
                .await
                .map_err(|e| format!("failed to materialize collection: {}", e))?;

            Ok(count.to_string())
        }
    }
}

/// Set migration for collection versions.
///
/// Sets the migration for all collections using the given source-destination
/// collection version IDs.
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
/// * `config` - JSON string of LensConfig containing:
///   - `source_version_id`: Source collection version ID
///   - `destination_version_id`: Destination collection version ID
///   - `lens`: Lens transform configuration
///
/// # Returns
///
/// - Status 0: Success (value contains the Lens transform ID)
/// - Status 1: Error (error field contains message)
///
/// # Safety
///
/// `config` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn set_migration(
    node_ptr: usize,
    identity_did: *const c_char,
    config: *const c_char,
) -> FfiResult {
    ffi_entry! { set(node_ptr, std::ptr::null(), identity_did, config) }
}

/// Set a migration in transaction `txn_id`; see [`set_migration`].
///
/// # Safety
///
/// Every string pointer must be null or a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn set_migration_in_txn(
    node_ptr: usize,
    txn_id: *const c_char,
    identity_did: *const c_char,
    config: *const c_char,
) -> FfiResult {
    ffi_entry! { set(node_ptr, txn_id, identity_did, config) }
}

/// Go's `SetMigration` through the core: the lens ID.
unsafe fn set(
    node_ptr: usize,
    txn_id: *const c_char,
    identity_did: *const c_char,
    config: *const c_char,
) -> FfiResult {
    let context = try_ffi!(crate::lens::call_context(identity_did, txn_id));
    let config_str = try_ffi!(require_c_str(config, "config"));
    let runner = try_ffi!(crate::helpers::get_node_runner(node_ptr));
    ffi_async!(caller, {
        runner
            .set_migration_as(&context, &config_str)
            .await
            .map_err(|e| e.to_string())
    })
}

/// Delete multiple collection versions by their version IDs.
///
/// Takes a JSON array of version ID strings. Versions are deleted in
/// topological order (children before parents).
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
/// * `version_ids_json` - JSON array of version ID strings
///
/// # Returns
///
/// - Status 0: Success (value is "{}")
/// - Status 1: Error (error field contains message)
///
/// # Safety
///
/// `version_ids_json` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn delete_collection_versions(
    node_ptr: usize,
    identity_did: *const c_char,
    version_ids_json: *const c_char,
) -> FfiResult {
    ffi_entry! {
        try_ffi!(check_nac_for_node(
            node_ptr,
            identity_did,
            NodePermission::CollectionPatch
        ));
        let ids_str = try_ffi!(require_c_str(version_ids_json, "version_ids_json"));
        let database = try_ffi!(get_node_database(node_ptr));

        // The caller acts in the call's context, so the NAC checks the body
        // reaches resolve the actual caller instead of the wildcard.
        let context = defra_core::RequestContext::default().with_identity(crate::types::c_str_to_string(identity_did).filter(|s| !s.is_empty()));

        ffi_async!(node_ptr, {
            let version_ids: Vec<String> = serde_json::from_str(&ids_str)
                .map_err(|e| format!("failed to parse version IDs JSON: {}", e))?;

            database
                .delete_collection_versions_batch_as(&context, version_ids)
                .await
                .map_err(|e| format!("failed to delete collection versions: {}", e))?;

            Ok("{}".to_string())
        })
    }
}
