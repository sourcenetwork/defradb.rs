use std::ffi::c_char;

use acp::nac::NodePermission;
use defra_p2p_adapter::peer;

use super::{json, ok, run, timeout};
use crate::target::{go_string, split_comma_separated};
use crate::types::DefraResult;
use crate::{ffi_entry, try_ffi};

/// Adds the comma-separated `collections` to the collections the node
/// replicates.
///
/// # Safety
///
/// `collections` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_collection_add(
    node_ptr: usize,
    collections: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let collections = split_comma_separated(&go_string(collections));
        ok(run(node_ptr, identity_ptr, NodePermission::P2pCollectionAdd, None, |ops| async move {
            peer::add_p2p_collections(&*ops, &collections).await
        }))
    }
}

/// Removes the comma-separated `collections` from the collections the node
/// replicates.
///
/// # Safety
///
/// `collections` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_collection_delete(
    node_ptr: usize,
    collections: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let collections = split_comma_separated(&go_string(collections));
        ok(run(node_ptr, identity_ptr, NodePermission::P2pCollectionDelete, None, |ops| async move {
            peer::delete_p2p_collections(&*ops, &collections).await
        }))
    }
}

/// Answers with the collections the node replicates as a JSON array.
#[no_mangle]
pub extern "C" fn defra_p2p_collection_list(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        json(run(node_ptr, identity_ptr, NodePermission::P2pCollectionList, None, |ops| async move {
            peer::list_p2p_collections(&*ops).await
        }))
    }
}

/// Synchronizes the branchable collection `collection_id` from connected
/// peers, within `timeout_str`, a Go duration (null or empty for none).
///
/// # Safety
///
/// `collection_id` and `timeout_str` must be null or valid null-terminated
/// strings.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_collection_sync_branchable(
    node_ptr: usize,
    collection_id: *const c_char,
    timeout_str: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let limit = try_ffi!(timeout(timeout_str));
        let collection_id = go_string(collection_id);
        ok(run(
            node_ptr,
            identity_ptr,
            NodePermission::P2pSyncBranchableCollection,
            limit,
            |ops| async move { ops.sync_branchable_collection(&collection_id).await },
        ))
    }
}

/// Synchronizes the comma-separated collection versions `version_ids` from
/// connected peers, within `timeout_str`, a Go duration (null or empty for
/// none).
///
/// # Safety
///
/// `version_ids` and `timeout_str` must be null or valid null-terminated
/// strings.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_collection_sync_versions(
    node_ptr: usize,
    version_ids: *const c_char,
    timeout_str: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let limit = try_ffi!(timeout(timeout_str));
        let version_ids = split_comma_separated(&go_string(version_ids));
        ok(run(
            node_ptr,
            identity_ptr,
            NodePermission::P2pSyncCollectionVersions,
            limit,
            |ops| async move { ops.sync_collection_versions(version_ids).await },
        ))
    }
}
