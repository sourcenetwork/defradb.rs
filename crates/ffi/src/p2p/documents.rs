use std::ffi::c_char;

use crate::ffi_entry;
use acp::nac::NodePermission;

use crate::helpers::{get_rt, require_c_str};
use crate::nac_check::check_nac_for_node;
use crate::state::NODES;
use crate::try_ffi;
use crate::types::FfiResult;
use defra_http::router::peer;

use super::{into_ffi_ok, into_ffi_result, parse_doc_ids_json, FfiP2PError};

/// Add tracked P2P documents to the node.
///
/// # Safety
///
/// `identity_did` and `doc_ids_json` must be valid null-terminated UTF-8 strings when non-null.
/// `node_ptr` must reference a live node handle created by this library.
#[no_mangle]
pub unsafe extern "C" fn p2p_add_documents(
    node_ptr: usize,
    identity_did: *const c_char,
    doc_ids_json: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let rt = try_ffi!(get_rt());
        try_ffi!(check_nac_for_node(
            node_ptr,
            identity_did,
            NodePermission::P2pDocumentAdd
        ));

        // The caller acts in the call's context, so the NAC checks the body
        // reaches resolve the actual caller instead of the wildcard.
        let context = defra_core::RequestContext::default().with_identity(crate::types::c_str_to_string(identity_did).filter(|s| !s.is_empty()));

        let doc_ids_str = try_ffi!(require_c_str(doc_ids_json, "doc_ids_json"));
        let doc_ids = match parse_doc_ids_json(&doc_ids_str) {
            Ok(doc_ids) => doc_ids,
            Err(error) => return FfiResult::error(error.message),
        };

        let result = NODES
            .get(node_ptr, |state| {
                let p2p = match &state.p2p {
                    Some(p2p) => p2p,
                    None => return Err(FfiP2PError::no_p2p_system()),
                };

                crate::helpers::block_on(rt, async {
                    peer::add_p2p_documents(&*p2p.system.ops_for(&context), &doc_ids)
                        .await
                        .map_err(FfiP2PError::from)
                })
            })
            .ok_or_else(FfiP2PError::invalid_node_handle)
            .and_then(|result| result);

        into_ffi_ok(result)
    }
}

/// Remove tracked P2P documents from the node.
///
/// # Safety
///
/// `identity_did` and `doc_ids_json` must be valid null-terminated UTF-8 strings when non-null.
/// `node_ptr` must reference a live node handle created by this library.
#[no_mangle]
pub unsafe extern "C" fn p2p_delete_documents(
    node_ptr: usize,
    identity_did: *const c_char,
    doc_ids_json: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let rt = try_ffi!(get_rt());
        try_ffi!(check_nac_for_node(
            node_ptr,
            identity_did,
            NodePermission::P2pDocumentDelete
        ));

        // The caller acts in the call's context, so the NAC checks the body
        // reaches resolve the actual caller instead of the wildcard.
        let context = defra_core::RequestContext::default().with_identity(crate::types::c_str_to_string(identity_did).filter(|s| !s.is_empty()));

        let doc_ids_str = try_ffi!(require_c_str(doc_ids_json, "doc_ids_json"));
        let doc_ids = match parse_doc_ids_json(&doc_ids_str) {
            Ok(doc_ids) => doc_ids,
            Err(error) => return FfiResult::error(error.message),
        };

        let result = NODES
            .get(node_ptr, |state| {
                let p2p = match &state.p2p {
                    Some(p2p) => p2p,
                    None => return Err(FfiP2PError::no_p2p_system()),
                };

                crate::helpers::block_on(rt, async {
                    peer::delete_p2p_documents(&*p2p.system.ops_for(&context), &doc_ids)
                        .await
                        .map_err(FfiP2PError::from)
                })
            })
            .ok_or_else(FfiP2PError::invalid_node_handle)
            .and_then(|result| result);

        into_ffi_ok(result)
    }
}

/// List tracked P2P documents for the node.
///
/// # Safety
///
/// `identity_did` must be a valid null-terminated UTF-8 string when non-null. `node_ptr` must
/// reference a live node handle created by this library.
#[no_mangle]
pub unsafe extern "C" fn p2p_list_documents(
    node_ptr: usize,
    identity_did: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let rt = try_ffi!(get_rt());
        try_ffi!(check_nac_for_node(
            node_ptr,
            identity_did,
            NodePermission::P2pDocumentList
        ));

        // The caller acts in the call's context, so the NAC checks the body
        // reaches resolve the actual caller instead of the wildcard.
        let context = defra_core::RequestContext::default().with_identity(crate::types::c_str_to_string(identity_did).filter(|s| !s.is_empty()));

        let result = NODES
            .get(node_ptr, |state| {
                let p2p = match &state.p2p {
                    Some(p2p) => p2p,
                    None => return Err(FfiP2PError::no_p2p_system()),
                };

                crate::helpers::block_on(rt, async {
                    let mut doc_ids =
                        peer::list_p2p_documents(&*p2p.system.ops_for(&context)).await?;
                    doc_ids.sort();
                    serde_json::to_string(&doc_ids)
                        .map_err(|error| {
                            FfiP2PError::internal(format!(
                                "failed to serialize documents: {}",
                                error
                            ))
                        })
                })
            })
            .ok_or_else(FfiP2PError::invalid_node_handle)
            .and_then(|result| result);

        into_ffi_result(result)
    }
}
