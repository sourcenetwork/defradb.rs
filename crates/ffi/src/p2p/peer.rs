use std::ffi::c_char;

use acp::nac::NodePermission;
use defra_p2p_adapter::peer;

use super::{json, ok, run};
use crate::ffi_entry;
use crate::target::{go_string, split_comma_separated};
use crate::types::DefraResult;

/// Connects to the comma-separated peer multiaddrs `peer_addresses`.
///
/// # Safety
///
/// `peer_addresses` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_connect(
    node_ptr: usize,
    peer_addresses: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let addresses = split_comma_separated(&go_string(peer_addresses));
        ok(run(node_ptr, identity_ptr, NodePermission::P2pPeerConnect, None, |ops| async move {
            peer::connect(&*ops, &addresses).await
        }))
    }
}

/// Disconnects from the comma-separated peer multiaddrs `peer_addresses`.
///
/// # Safety
///
/// `peer_addresses` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_disconnect(
    node_ptr: usize,
    peer_addresses: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let addresses = split_comma_separated(&go_string(peer_addresses));
        ok(run(node_ptr, identity_ptr, NodePermission::P2pPeerDisconnect, None, |ops| async move {
            peer::disconnect(&*ops, &addresses).await
        }))
    }
}

/// Answers with the addresses other peers reach this node at, as a JSON
/// array: each multiaddr with this node's peer ID appended.
#[no_mangle]
pub extern "C" fn defra_p2p_info(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        json(run(node_ptr, identity_ptr, NodePermission::P2pPeerInfo, None, |ops| async move {
            peer::peer_info(&*ops).await
        }))
    }
}

/// Answers with the peers the node is connected to as a JSON array.
#[no_mangle]
pub extern "C" fn defra_p2p_peer_list(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        json(run(node_ptr, identity_ptr, NodePermission::P2pPeerActive, None, |ops| async move {
            peer::active_peers(&*ops).await
        }))
    }
}
