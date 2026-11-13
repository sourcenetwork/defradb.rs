use std::ffi::c_char;

use acp::nac::NodePermission;
use defra_p2p_adapter::peer;

use super::{json, ok, run, timeout};
use crate::target::{go_string, split_comma_separated};
use crate::types::DefraResult;
use crate::{ffi_entry, try_ffi};

/// Adds the comma-separated document IDs `doc_ids` to the documents the
/// node replicates.
///
/// # Safety
///
/// `doc_ids` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_document_add(
    node_ptr: usize,
    doc_ids: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let doc_ids = split_comma_separated(&go_string(doc_ids));
        ok(run(node_ptr, identity_ptr, NodePermission::P2pDocumentAdd, None, |ops| async move {
            peer::add_p2p_documents(&*ops, &doc_ids).await
        }))
    }
}

/// Removes the comma-separated document IDs `doc_ids` from the documents the
/// node replicates.
///
/// # Safety
///
/// `doc_ids` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_document_delete(
    node_ptr: usize,
    doc_ids: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let doc_ids = split_comma_separated(&go_string(doc_ids));
        ok(run(node_ptr, identity_ptr, NodePermission::P2pDocumentDelete, None, |ops| async move {
            peer::delete_p2p_documents(&*ops, &doc_ids).await
        }))
    }
}

/// Answers with the IDs of the documents the node replicates as a JSON
/// array.
#[no_mangle]
pub extern "C" fn defra_p2p_document_list(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        json(run(node_ptr, identity_ptr, NodePermission::P2pDocumentList, None, |ops| async move {
            peer::list_p2p_documents(&*ops).await
        }))
    }
}

/// Synchronizes the comma-separated documents `doc_ids` of collection
/// `collection` from connected peers, within `timeout_str`, a Go duration
/// (null or empty for none).
///
/// # Safety
///
/// Every string must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_document_sync(
    node_ptr: usize,
    collection: *const c_char,
    doc_ids: *const c_char,
    timeout_str: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let limit = try_ffi!(timeout(timeout_str));
        let collection = go_string(collection);
        let doc_ids = split_comma_separated(&go_string(doc_ids));
        ok(run(node_ptr, identity_ptr, NodePermission::P2pSyncDocuments, limit, |ops| async move {
            ops.sync_documents(&collection, doc_ids, limit).await
        }))
    }
}
