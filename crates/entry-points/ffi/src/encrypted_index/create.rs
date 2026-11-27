use std::ffi::c_char;

use acp::nac::NodePermission;

use crate::request::{on_scope, Request};
use crate::target::go_string;
use crate::types::DefraResult;
use crate::{ffi_async, ffi_entry, try_ffi};

/// Go's encrypted index type names; empty is the default, equality.
fn index_type(name: &str) -> Result<schema::EncryptedIndexType, String> {
    match name {
        "" | "equality" => Ok(schema::EncryptedIndexType::Equality),
        other => Err(format!("unsupported encrypted index type: {}", other)),
    }
}

/// Creates an encrypted index of type `index_type` ("equality", the default
/// for null or empty) on field `field_name` of collection `collection_name`,
/// and answers with its description as JSON.
///
/// # Safety
///
/// Every string must be null or a valid null-terminated string.
pub unsafe fn defra_encrypted_index_new(
    node_ptr: usize,
    collection_name: *const c_char,
    field_name: *const c_char,
    index_type_name: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::EncryptedIndexAdd));
        try_ffi!(index_type(&go_string(index_type_name)).map_err(DefraResult::error));
        let collection_name = go_string(collection_name);
        let field_name = go_string(field_name);
        let node = try_ffi!(request.node_handle());
        let scope = try_ffi!(request.node_scope());

        ffi_async!(caller, {
            let index = on_scope!(node, &scope, |scoped| {
                scoped
                    .add_encrypted_index(&collection_name, &field_name)
                    .await
            })
            .map_err(|e| e.to_string())?;
            serde_json::to_string(&index).map_err(|e| format!("failed to serialize result: {}", e))
        })
    }
}

/// Deletes the encrypted index on field `field_name` of collection
/// `collection_name`.
///
/// # Safety
///
/// Every string must be null or a valid null-terminated string.
pub unsafe fn defra_encrypted_index_delete(
    node_ptr: usize,
    collection_name: *const c_char,
    field_name: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::EncryptedIndexDelete));
        let collection_name = go_string(collection_name);
        let field_name = go_string(field_name);
        let node = try_ffi!(request.node_handle());
        let scope = try_ffi!(request.node_scope());

        ffi_async!(caller, {
            on_scope!(node, &scope, |scoped| {
                scoped
                    .delete_encrypted_index(&collection_name, &field_name)
                    .await
            })
            .map_err(|e| e.to_string())?;
            Ok("{}".to_string())
        })
    }
}
