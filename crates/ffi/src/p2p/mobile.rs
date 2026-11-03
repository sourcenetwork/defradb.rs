//! P2P calls for an app on a phone, which Go's cbindings have no
//! counterpart for.

use std::ffi::c_char;

use acp::nac::NodePermission;

use super::{admit, drive, json, ok, run};
use crate::ffi_entry;
use crate::target::go_string;
use crate::types::DefraResult;

/// Answers with the best address to share with another peer so it can dial
/// this node, as a JSON string, or JSON `null` while there is none yet (the
/// node runs without P2P, or its transport has no dialable address).
#[no_mangle]
pub extern "C" fn defra_mobile_shareable_address(
    node_ptr: usize,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        match admit(node_ptr, identity_ptr, NodePermission::P2pPeerInfo) {
            Ok(Some(ops)) => json(drive(ops, None, |ops| async move { ops.shareable_address().await })),
            Ok(None) => DefraResult::success("null"),
            Err(error) => error,
        }
    }
}

/// Answers with the node's live P2P sync status as JSON: its outbound push
/// backlog, the DAGs it is still fetching, and its retry and overload
/// counters.
#[no_mangle]
pub extern "C" fn defra_mobile_sync_status(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        json(run(node_ptr, identity_ptr, NodePermission::P2pPeerInfo, None, |ops| async move {
            ops.sync_status().await
        }))
    }
}

/// Answers with everything the node knows about the peer `peer_id`, a raw
/// transport peer ID, as JSON: whether it is connected, what was pushed to
/// it, and its retry state.
///
/// # Safety
///
/// `peer_id` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_mobile_peer_details(
    node_ptr: usize,
    peer_id: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let peer_id = match defra_p2p_adapter::TransportPeerId::new(go_string(peer_id)) {
            Ok(peer_id) => peer_id,
            Err(error) => return DefraResult::error(error.to_string()),
        };
        json(run(node_ptr, identity_ptr, NodePermission::P2pPeerInfo, None, |ops| async move {
            ops.peer_details(&peer_id).await
        }))
    }
}

/// Tells the node its network may have changed (Wi-Fi to cellular, a new
/// address, waking up), so the iroh transport looks for new paths to its
/// peers at once instead of waiting to notice. With libp2p it does nothing.
#[no_mangle]
pub extern "C" fn defra_mobile_notify_network_change(
    node_ptr: usize,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        ok(run(node_ptr, identity_ptr, NodePermission::P2pPeerConnect, None, |ops| async move {
            ops.notify_network_change().await
        }))
    }
}
