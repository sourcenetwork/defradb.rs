use std::ffi::c_char;

use acp::nac::NodePermission;
use defra_p2p_adapter::peer;

use super::{json, ok, run};
use crate::ffi_entry;
use crate::target::{go_string, non_empty, split_comma_separated};
use crate::types::DefraResult;

/// Replicates the comma-separated `collections` to the peer the
/// comma-separated multiaddrs `addresses` name.
///
/// # Safety
///
/// `collections` and `addresses` must be null or valid null-terminated
/// strings.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_replicator_add(
    node_ptr: usize,
    collections: *const c_char,
    addresses: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let collections = split_comma_separated(&go_string(collections));
        let addresses = split_comma_separated(&go_string(addresses));
        ok(run(node_ptr, identity_ptr, NodePermission::P2pReplicatorAdd, None, |ops| async move {
            peer::add_replicator(&*ops, &addresses, &collections, Default::default()).await
        }))
    }
}

/// Stops replicating the comma-separated `collections` (every collection for
/// null or empty) to the replicator `id`.
///
/// # Safety
///
/// `collections` and `id` must be null or valid null-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_replicator_delete(
    node_ptr: usize,
    collections: *const c_char,
    id: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let collections = split_comma_separated(&go_string(collections));
        let id = non_empty(id);
        ok(run(node_ptr, identity_ptr, NodePermission::P2pReplicatorDelete, None, |ops| async move {
            ops.remove_replicator(collections, id.as_deref()).await
        }))
    }
}

/// Answers with the node's replicators as a JSON array of Go's
/// `client.Replicator`.
#[no_mangle]
pub extern "C" fn defra_p2p_replicator_list(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        json(run(node_ptr, identity_ptr, NodePermission::P2pReplicatorList, None, |ops| async move {
            peer::list_replicators(&*ops).await
        }))
    }
}
