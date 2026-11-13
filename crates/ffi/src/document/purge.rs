use std::ffi::c_char;

use acp::nac::NodePermission;

use super::run::DocumentCall;
use crate::state::NODES;
use crate::target::go_string;
use crate::types::{CollectionOptions, DefraResult};
use crate::{ffi_async, ffi_entry, try_ffi, ERR_INVALID_NODE_HANDLE};

/// Permanently removes the documents the JSON array `doc_ids` names from the
/// collection `opts` selects, on this node only, replicating nothing: their
/// values, index entries, heads and DocID aliases, and their history unless
/// the collection is branchable. A DocID that names no document is skipped.
/// In a transaction, nothing is removed until it commits. Answers with
/// `{"Count": ...}`, the documents purged. Every version of a collection
/// shares its documents, so which version `opts` selects does not change
/// what is purged.
///
/// # Safety
///
/// `doc_ids` must be null or a valid null-terminated string; every non-null
/// string in `opts` must be one too.
#[no_mangle]
pub unsafe extern "C" fn defra_document_purge(
    node_ptr: usize,
    doc_ids: *const c_char,
    opts: CollectionOptions,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let ids: Vec<String> = try_ffi!(serde_json::from_str(go_string(doc_ids).trim())
            .map_err(|e| DefraResult::error(format!("invalid document IDs JSON array: {}", e))));
        let ids = try_ffi!(ids
            .iter()
            .map(|id| document::DocID::from_string(id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| DefraResult::error(e.to_string())));
        let call = try_ffi!(DocumentCall::begin(node_ptr, &opts, identity_ptr, NodePermission::CollectionTruncate));
        let target = call.target().clone();
        let registry = try_ffi!(NODES
            .get(target.node(), |state| state._node.txn_registry().clone())
            .ok_or_else(|| DefraResult::error(ERR_INVALID_NODE_HANDLE)));
        ffi_async!(target &target, {
            let name = call.collection().await?.name;
            let purged = registry
                .purge_by_doc_ids_as(&call.context(), &name, &ids)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "Count": purged }).to_string())
        })
    }
}
