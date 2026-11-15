use std::ffi::{c_char, c_int};

use acp::nac::NodePermission;

use super::run::DocumentCall;
use crate::target::go_string;
use crate::types::{CollectionOptions, DefraResult};
use crate::{ffi_async, ffi_entry, try_ffi};

/// Saves each document of the JSON array `json_data`, in order, to the
/// collection `opts` selects: one whose `_docID` names a document updates
/// it, any other is added. Every document is stored or none is. Answers with
/// the document IDs as a JSON array, in the documents' order.
/// `is_encrypted` encrypts every field of an added document; otherwise
/// `encrypted_fields`, a comma-separated list, names the fields to encrypt.
///
/// # Safety
///
/// `json_data` and `encrypted_fields` must be null or valid null-terminated
/// strings; every non-null string in `opts` must be one too.
#[no_mangle]
pub unsafe extern "C" fn defra_document_save_many(
    node_ptr: usize,
    json_data: *const c_char,
    is_encrypted: c_int,
    encrypted_fields: *const c_char,
    opts: CollectionOptions,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let call = try_ffi!(DocumentCall::begin(node_ptr, &opts, identity_ptr, NodePermission::DocumentUpdate));
        let json_data = go_string(json_data);
        let options = query::documents::AddOptions {
            encrypt_doc: is_encrypted != 0,
            encrypted_fields: go_string(encrypted_fields)
                .split(',')
                .map(str::trim)
                .filter(|field| !field.is_empty())
                .map(str::to_string)
                .collect(),
            // The call's signer already holds `opts.enable_signing` (DocumentCall::begin).
            enable_signing: None,
        };
        let target = call.target().clone();
        ffi_async!(target &target, {
            let docs: Vec<serde_json::Value> = serde_json::from_str(json_data.trim())
                .map_err(|e| format!("invalid documents JSON array: {}", e))?;
            let name = call.collection().await?.name;
            let ids = query::documents::save_many(|request| call.execute(request), &name, &docs, &options)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_string(&ids).map_err(|e| e.to_string())
        })
    }
}
