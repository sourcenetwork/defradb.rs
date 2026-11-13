use std::ffi::c_char;

use acp::nac::NodePermission;

use crate::request::Request;
use crate::target::non_empty;
use crate::types::DefraResult;
use crate::{ffi_async, ffi_entry, try_ffi};

/// Answers with the encrypted indexes of collection `collection_name` as a
/// JSON array, or, for null or empty, every collection's as a JSON object
/// keyed by collection name.
///
/// # Safety
///
/// `collection_name` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_encrypted_index_list(
    node_ptr: usize,
    collection_name: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let permission = match non_empty(collection_name) {
            Some(_) => NodePermission::EncryptedIndexList,
            None => NodePermission::EncryptedIndexListAll,
        };
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, permission));
        try_ffi!(request.require_node("encrypted index list"));
        let database = try_ffi!(request.database());

        match non_empty(collection_name) {
            Some(collection_name_str) => ffi_async!(caller, {
            let collection = database
                .get_collection(&collection_name_str)
                .map_err(|e| format!("failed to get collection: {}", e))?
                .ok_or_else(|| format!("collection '{}' not found", collection_name_str))?;

            let encrypted_indexes = &collection.schema().encrypted_indexes;

            let json = serde_json::to_string(encrypted_indexes)
                .map_err(|e| format!("failed to serialize result: {}", e))?;

            Ok(json)
            }),
            None => ffi_async!(caller, {
            let names = database
                .list_collections()
                .map_err(|e| format!("failed to list collections: {}", e))?;

            let mut all_encrypted_indexes: rapidhash::RapidHashMap<
                String,
                Vec<schema::EncryptedIndexDescription>,
            > = rapidhash::RapidHashMap::default();

            for name in names {
                match database.get_collection(&name) {
                    Ok(Some(collection)) => {
                        let encrypted_indexes = collection.schema().encrypted_indexes.clone();
                        if !encrypted_indexes.is_empty() {
                            all_encrypted_indexes.insert(name, encrypted_indexes);
                        }
                    }
                    Ok(None) => continue,
                    Err(e) => {
                        return Err(format!("failed to get collection '{}': {}", name, e));
                    }
                }
            }

            let json = serde_json::to_string(&all_encrypted_indexes)
                .map_err(|e| format!("failed to serialize result: {}", e))?;

            Ok(json)
            }),
        }
    }
}
