use std::ffi::c_char;
use std::sync::Arc;

use acp::nac::NodePermission;
use defra_p2p_adapter::peer;

use super::{json, ok, run, ERR_NO_P2P};
use crate::ffi_entry;
use crate::request::Request;
use crate::state::NODES;
use crate::target::{go_string, non_empty, split_comma_separated, ERR_INVALID_HANDLE};
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

/// Replicates the comma-separated `collections` to the peer the
/// comma-separated multiaddrs `addresses` name, pushing only the documents
/// `filters_json` selects: a JSON object keyed by collection name, each value
/// a replication filter as HTTP's `"Filters"` field takes it. Null, empty or
/// `{}` filters nothing, as [`defra_p2p_replicator_add`] does.
///
/// # Safety
///
/// `collections`, `addresses` and `filters_json` must be null or valid
/// null-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_replicator_add_with_filter(
    node_ptr: usize,
    collections: *const c_char,
    addresses: *const c_char,
    filters_json: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let collections = split_comma_separated(&go_string(collections));
        let addresses = split_comma_separated(&go_string(addresses));
        let filters = match parse_filters(filters_json) {
            Ok(filters) => filters,
            Err(error) => return DefraResult::error(error),
        };
        ok(run(node_ptr, identity_ptr, NodePermission::P2pReplicatorAdd, None, |ops| async move {
            let extras = peer::ReplicatorExtras {
                filters,
                ..Default::default()
            };
            peer::add_replicator(&*ops, &addresses, &collections, extras).await
        }))
    }
}

/// The replication filters `filters_json` names; none for null or empty.
///
/// # Safety
///
/// `filters_json` must be null or a valid null-terminated string.
unsafe fn parse_filters(
    filters_json: *const c_char,
) -> Result<defra_p2p_adapter::ReplicationFilters, String> {
    match non_empty(filters_json) {
        None => Ok(defra_p2p_adapter::ReplicationFilters::new()),
        Some(json) => serde_json::from_str::<Option<defra_p2p_adapter::ReplicationFilters>>(&json)
            .map(Option::unwrap_or_default)
            .map_err(|error| format!("invalid filters_json: {}", error)),
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

/// Runs one retry pass over the node's replicators, as Go's
/// `RetryReplicators`: re-pushes the document blocks a replicator missed and
/// regenerates and re-pushes their searchable-encryption artifacts. Admitted
/// as `add-p2p-replicator`, since it does the pushing adding one starts.
#[no_mangle]
pub extern "C" fn defra_p2p_replicator_retry(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        ok(retry(node_ptr, identity_ptr))
    }
}

fn retry(node_ptr: usize, identity_ptr: usize) -> Result<(), DefraResult> {
    let request = Request::begin(node_ptr, identity_ptr, NodePermission::P2pReplicatorAdd)?;
    let node = request.require_node("P2P")?;
    let system = NODES
        .get(node, |state| {
            state.p2p.as_ref().map(|p2p| Arc::clone(&p2p.system))
        })
        .ok_or_else(|| DefraResult::error(ERR_INVALID_HANDLE))?
        .ok_or_else(|| DefraResult::error(ERR_NO_P2P))?;
    let rt = crate::helpers::get_rt()?;
    match rt.block_on(system.retry_replicators()) {
        Ok(result) => result.map_err(DefraResult::error),
        Err(error) => Err(DefraResult::error(error.to_string())),
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
