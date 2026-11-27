use std::ffi::c_char;

use acp::nac::NodePermission;

use crate::request::{on_scope, Request};
use crate::target::non_empty;
use crate::types::DefraResult;
use crate::{ffi_async, ffi_entry, try_ffi};

/// Answers with the encrypted indexes of collection `collection_name` as a
/// JSON array, or, for null or empty, every collection's as a JSON object
/// keyed by collection name, leaving out collections without any.
///
/// # Safety
///
/// `collection_name` must be null or a valid null-terminated string.
pub unsafe fn defra_encrypted_index_list(
    node_ptr: usize,
    collection_name: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let collection_name = non_empty(collection_name);
        let permission = match collection_name {
            Some(_) => NodePermission::EncryptedIndexList,
            None => NodePermission::EncryptedIndexListAll,
        };
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, permission));
        let node = try_ffi!(request.node_handle());
        let scope = try_ffi!(request.node_scope());

        ffi_async!(caller, {
            let json = match collection_name {
                Some(name) => {
                    let indexes = on_scope!(node, &scope, |scoped| {
                        scoped.list_encrypted_indexes(&name).await
                    })
                    .map_err(|e| e.to_string())?;
                    serde_json::to_string(&indexes)
                }
                None => {
                    let all: rapidhash::RapidHashMap<_, _> = on_scope!(node, &scope, |scoped| {
                        scoped.list_all_encrypted_indexes().await
                    })
                    .map_err(|e| e.to_string())?
                        .into_iter()
                        .collect();
                    serde_json::to_string(&all)
                }
            };
            json.map_err(|e| format!("failed to serialize result: {}", e))
        })
    }
}
