use std::ffi::{c_char, c_int};

use acp::nac::NodePermission;

use super::run::DocumentCall;
use crate::target::go_string;
use crate::types::{CollectionOptions, DefraResult};
use crate::{ffi_async, ffi_entry, try_ffi};

/// Adds a JSON document, or a JSON array of documents, to the collection
/// `opts` selects, and answers with the new document IDs as a JSON array.
/// `is_encrypted` encrypts every field; otherwise `encrypted_fields`, a
/// comma-separated list, names the fields to encrypt.
///
/// # Safety
///
/// `json_data` and `encrypted_fields` must be null or valid null-terminated
/// strings; every non-null string in `opts` must be one too.
#[no_mangle]
pub unsafe extern "C" fn defra_document_add(
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
        let encrypt = is_encrypted != 0;
        let encrypt_fields: Vec<String> = go_string(encrypted_fields)
            .split(',')
            .map(str::trim)
            .filter(|field| !field.is_empty())
            .map(str::to_string)
            .collect();
        let target = call.target().clone();
        ffi_async!(target &target, {
            let data: serde_json::Value = serde_json::from_str(json_data.trim())
                .map_err(|e| format!("invalid document JSON: {}", e))?;
            let docs = match data {
                serde_json::Value::Array(docs) => docs,
                doc => vec![doc],
            };
            let name = call.collection().await?.name;
            let options = query::documents::AddOptions {
                encrypt_doc: encrypt,
                encrypted_fields: encrypt_fields,
                // The call's signer already holds `opts.enable_signing` (DocumentCall::begin).
                enable_signing: None,
            };
            let ids = query::documents::add(|request| call.execute(request), &name, &docs, &options)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_string(&ids).map_err(|e| e.to_string())
        })
    }
}
