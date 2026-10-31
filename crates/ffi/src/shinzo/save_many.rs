//! Save many documents in one transaction (Go Shinzo's `SaveManyDocuments`).

use std::ffi::c_char;

use acp::nac::NodePermission;
use serde_json::{json, Value};

use crate::helpers::require_c_str;
use crate::types::{c_str_to_string, FfiResult};
use crate::{ffi_async, ffi_entry, try_ffi};

/// Save each document in order, updating the one its `_docID` names or
/// adding it otherwise, all in one transaction: every document is stored or
/// none is.
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
/// * `txn_id` - Optional transaction ID (nullable): the batch lands when it commits
/// * `identity_did` - Optional identity DID (nullable)
/// * `collection_name` - The collection to save into
/// * `docs_json` - JSON array of document objects
/// * `options_json` - Optional (nullable) `{"encryptDoc": bool, "encryptedFields": [..]}`
/// * `batch_session_id` - Optional (nullable) batch-signing session the writes are collected in
///
/// # Returns
///
/// - Status 0: Success (value is `{"docIDs": [...]}`, in the documents' order)
/// - Status 1: Error (error field contains message)
///
/// # Safety
///
/// `collection_name` and `docs_json` must be valid null-terminated UTF-8
/// strings; the nullable arguments null or such strings.
#[no_mangle]
pub unsafe extern "C" fn save_many_documents(
    node_ptr: usize,
    txn_id: *const c_char,
    identity_did: *const c_char,
    collection_name: *const c_char,
    docs_json: *const c_char,
    options_json: *const c_char,
    batch_session_id: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let collection = try_ffi!(require_c_str(collection_name, "collection_name"));
        let docs = try_ffi!(require_c_str(docs_json, "docs_json"));
        let docs: Vec<Value> = try_ffi!(serde_json::from_str(&docs)
            .map_err(|e| FfiResult::error(format!("invalid docs_json: {e}"))));
        let options = try_ffi!(add_options(c_str_to_string(options_json)));
        let identity = c_str_to_string(identity_did).filter(|did| !did.is_empty());
        let txn = c_str_to_string(txn_id).filter(|id| !id.is_empty());
        let batch_session = c_str_to_string(batch_session_id).filter(|id| !id.is_empty());

        ffi_async!(caller, {
            let run = |request| {
                crate::query::run_typed(node_ptr, identity, txn, batch_session, NodePermission::DocumentUpdate, request)
            };
            let doc_ids = query::documents::save_many(run, &collection, &docs, &options)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({ "docIDs": doc_ids }).to_string())
        })
    }
}

fn add_options(json: Option<String>) -> Result<query::documents::AddOptions, FfiResult> {
    #[derive(serde::Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    struct Options {
        encrypt_doc: bool,
        encrypted_fields: Vec<String>,
    }
    let options: Options = match json.filter(|json| !json.is_empty()) {
        Some(json) => serde_json::from_str(&json)
            .map_err(|e| FfiResult::error(format!("invalid options_json: {e}")))?,
        None => Options::default(),
    };
    Ok(query::documents::AddOptions {
        encrypt_doc: options.encrypt_doc,
        encrypted_fields: options.encrypted_fields,
    })
}
