//! Purge by DocID (Go Shinzo's `PurgeByDocIDs`).

use std::ffi::c_char;

use serde_json::json;

use acp::nac::NodePermission;

use crate::helpers::require_c_str;
use crate::state::NODES;
use crate::types::{c_str_to_string, FfiResult};
use crate::{ffi_async, ffi_entry, try_ffi};

/// Permanently remove documents from this node by DocID, replicating
/// nothing: their values, index entries, heads and DocID aliases, and with
/// `prune_history` every block of their history no other document owns.
/// A DocID that names no document is skipped.
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
/// * `txn_id` - Optional transaction ID (nullable): the purge lands when it commits
/// * `identity_did` - Optional identity DID (nullable)
/// * `collection_name` - The collection to purge from
/// * `doc_ids_json` - JSON array of DocID strings
/// * `prune_history` - Also delete the documents' history blocks
///
/// # Returns
///
/// - Status 0: Success (value is `{"purged": N}`, the documents purged)
/// - Status 1: Error (error field contains message)
///
/// # Safety
///
/// `collection_name` and `doc_ids_json` must be valid null-terminated UTF-8
/// strings; `txn_id` and `identity_did` null or such strings.
#[no_mangle]
pub unsafe extern "C" fn purge_documents(
    node_ptr: usize,
    txn_id: *const c_char,
    identity_did: *const c_char,
    collection_name: *const c_char,
    doc_ids_json: *const c_char,
    prune_history: bool,
) -> FfiResult {
    ffi_entry! {
        let collection = try_ffi!(require_c_str(collection_name, "collection_name"));
        let doc_ids = try_ffi!(require_c_str(doc_ids_json, "doc_ids_json"));
        let doc_ids: Vec<String> = try_ffi!(serde_json::from_str(&doc_ids)
            .map_err(|e| FfiResult::error(format!("invalid doc_ids_json: {e}"))));
        let doc_ids = try_ffi!(doc_ids
            .iter()
            .map(|doc_id| document::DocID::from_string(doc_id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| FfiResult::error(format!("invalid DocID: {e}"))));
        let identity = c_str_to_string(identity_did).filter(|did| !did.is_empty());
        let txn = c_str_to_string(txn_id).filter(|id| !id.is_empty());
        let in_txn = txn.is_some();
        let registry = try_ffi!(NODES
            .get(node_ptr, |state| state._node.txn_registry().clone())
            .ok_or_else(|| FfiResult::error(crate::ERR_INVALID_NODE_HANDLE)));
        let purge = async move {
            let context = crate::query::caller_context(node_ptr, identity, txn, NodePermission::CollectionTruncate).await?;
            let purged = registry
                .purge_by_doc_ids_as(&context, &collection, &doc_ids, prune_history)
                .await
                .map_err(|e| e.to_string())?;
            Ok::<String, String>(json!({ "purged": purged }).to_string())
        };
        match in_txn {
            // An explicit transaction's call routes itself to its owner.
            true => ffi_async!(caller, { purge.await }),
            false => ffi_async!(node_ptr, { purge.await }),
        }
    }
}
