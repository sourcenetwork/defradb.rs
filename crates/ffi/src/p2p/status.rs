use std::ffi::c_char;

use acp::nac::NodePermission;

use crate::ffi_entry;
use crate::helpers::{get_rt, require_c_str};
use crate::nac_check::check_nac_for_node;
use crate::state::NODES;
use crate::try_ffi;
use crate::types::FfiResult;

use super::{into_ffi_result, FfiP2PError};

/// Return the node's live P2P sync status as JSON.
///
/// # Safety
///
/// `identity_did` must be a valid null-terminated UTF-8 string when non-null.
/// `node_ptr` must reference a live node handle created by this library.
#[no_mangle]
pub unsafe extern "C" fn p2p_sync_status(
    node_ptr: usize,
    identity_did: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let rt = try_ffi!(get_rt());
        try_ffi!(check_nac_for_node(
            node_ptr,
            identity_did,
            NodePermission::P2pPeerInfo
        ));

        let context = defra_core::RequestContext::default().with_identity(crate::types::c_str_to_string(identity_did).filter(|s| !s.is_empty()));

        let result = NODES
            .get(node_ptr, |state| state.p2p.clone())
            .ok_or_else(FfiP2PError::invalid_node_handle)
            .and_then(|p2p| p2p.ok_or_else(FfiP2PError::no_p2p_system))
            .and_then(|p2p| {
                crate::helpers::block_on(rt, async {
                    let status = p2p
                        .system
                        .ops_for(&context)
                        .sync_status()
                        .await
                        .map_err(FfiP2PError::from)?;
                    serde_json::to_string(&status).map_err(|error| {
                        FfiP2PError::internal(format!(
                            "failed to serialize P2P sync status: {}",
                            error
                        ))
                    })
                })
            });

        into_ffi_result(result)
    }
}

/// Return everything the node knows about one peer as JSON.
///
/// # Safety
///
/// `identity_did` must be a valid null-terminated UTF-8 string when non-null.
/// `peer_id` must be a valid null-terminated UTF-8 raw transport peer ID.
/// `node_ptr` must reference a live node handle created by this library.
#[no_mangle]
pub unsafe extern "C" fn p2p_peer_details(
    node_ptr: usize,
    identity_did: *const c_char,
    peer_id: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let rt = try_ffi!(get_rt());
        try_ffi!(check_nac_for_node(
            node_ptr,
            identity_did,
            NodePermission::P2pPeerInfo
        ));

        let context = defra_core::RequestContext::default().with_identity(crate::types::c_str_to_string(identity_did).filter(|s| !s.is_empty()));
        let peer_id = try_ffi!(require_c_str(peer_id, "peer_id"));

        let result = defra_http::router::TransportPeerId::new(peer_id)
            .map_err(FfiP2PError::from)
            .and_then(|peer_id| {
                NODES
                    .get(node_ptr, |state| state.p2p.clone())
                    .ok_or_else(FfiP2PError::invalid_node_handle)
                    .and_then(|p2p| p2p.ok_or_else(FfiP2PError::no_p2p_system))
                    .map(|p2p| (p2p, peer_id))
            })
            .and_then(|(p2p, peer_id)| {
                crate::helpers::block_on(rt, async {
                    let details = p2p
                        .system
                        .ops_for(&context)
                        .peer_details(&peer_id)
                        .await
                        .map_err(FfiP2PError::from)?;
                    serde_json::to_string(&details).map_err(|error| {
                        FfiP2PError::internal(format!(
                            "failed to serialize P2P peer details: {}",
                            error
                        ))
                    })
                })
            });

        into_ffi_result(result)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "libp2p")]
    use std::ffi::{CStr, CString};
    use std::ptr;

    use crate::node::{new_node, node_close};
    #[cfg(feature = "libp2p")]
    use crate::p2p::new_node_with_p2p;
    use crate::types::{defra_free_string, NodeInitOptions};

    use super::*;

    #[test]
    fn p2p_sync_status_exports_peer_info_style_ffi_signature() {
        let symbol: unsafe extern "C" fn(usize, *const c_char) -> FfiResult = p2p_sync_status;
        let _ = symbol;
    }

    #[cfg(feature = "libp2p")]
    #[test]
    fn p2p_sync_status_returns_live_snapshot() {
        assert!(crate::runtime::init_runtime(), "runtime init must succeed");
        let listen_addr = CString::new("/ip4/127.0.0.1/tcp/0").unwrap();
        let result = unsafe { new_node_with_p2p(NodeInitOptions::default(), listen_addr.as_ptr()) };
        assert_eq!(result.status, 0, "P2P node creation must succeed");

        let status = unsafe { p2p_sync_status(result.node_ptr, ptr::null()) };
        assert_eq!(status.status, 0, "sync status lookup must succeed");
        let value: serde_json::Value = unsafe {
            serde_json::from_str(&CStr::from_ptr(status.value).to_string_lossy()).unwrap()
        };
        assert!(value.get("push_backlog").is_some());

        unsafe { defra_free_string(status.value) };
        node_close(result.node_ptr);
    }

    #[test]
    fn p2p_peer_details_exports_peer_info_style_ffi_signature() {
        let symbol: unsafe extern "C" fn(usize, *const c_char, *const c_char) -> FfiResult =
            p2p_peer_details;
        let _ = symbol;
    }

    #[cfg(feature = "libp2p")]
    #[test]
    fn p2p_peer_details_returns_json_for_a_peer() {
        assert!(crate::runtime::init_runtime(), "runtime init must succeed");
        let listen_addr = CString::new("/ip4/127.0.0.1/tcp/0").unwrap();
        let result = unsafe { new_node_with_p2p(NodeInitOptions::default(), listen_addr.as_ptr()) };
        assert_eq!(result.status, 0, "P2P node creation must succeed");
        let peer = CString::new("12D3KooWD3eckifWpRn9wQpMG9R9hX3sD158z7EqHWmweQAJU5SA").unwrap();

        let details = unsafe { p2p_peer_details(result.node_ptr, ptr::null(), peer.as_ptr()) };
        assert_eq!(details.status, 0, "peer details lookup must succeed");
        let value: serde_json::Value = unsafe {
            serde_json::from_str(&CStr::from_ptr(details.value).to_string_lossy()).unwrap()
        };
        assert_eq!(value["peer_id"], peer.to_str().unwrap());
        assert_eq!(value["connected"], false);
        assert!(value.get("heads_pushed").is_some());
        unsafe { defra_free_string(details.value) };

        let invalid = CString::new("/ip4/127.0.0.1").unwrap();
        let rejected = unsafe { p2p_peer_details(result.node_ptr, ptr::null(), invalid.as_ptr()) };
        assert_eq!(rejected.status, 1);
        unsafe { defra_free_string(rejected.error) };
        node_close(result.node_ptr);
    }

    #[test]
    fn p2p_sync_status_requires_p2p() {
        assert!(crate::runtime::init_runtime(), "runtime init must succeed");
        let result = new_node(NodeInitOptions::default());
        assert_eq!(result.status, 0, "node creation must succeed");

        let status = unsafe { p2p_sync_status(result.node_ptr, ptr::null()) };
        assert_eq!(status.status, 1);
        assert!(!status.error.is_null());

        unsafe { defra_free_string(status.error) };
        node_close(result.node_ptr);
    }
}
