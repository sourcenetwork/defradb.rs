use std::ffi::c_char;

use serde_json::Value as JsonValue;

use crate::helpers::require_c_str;
use crate::types::{c_str_to_string, FfiResult};
use crate::{ffi_async, ffi_entry, try_ffi};

/// Add one document, or a JSON array of them, to a collection, through the
/// core's document writes on the node's request path.
///
/// Returns the JSON array of the added document IDs, in input order, as Go's
/// `POST /collections/{name}` does.
///
/// # Safety
///
/// All string pointers must be valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn collection_create(
    node_ptr: usize,
    identity_did: *const c_char,
    collection_name: *const c_char,
    json_data: *const c_char,
    batch_session_id: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let collection = try_ffi!(require_c_str(collection_name, "collection_name"));
        let json_str = try_ffi!(require_c_str(json_data, "json_data"));
        let identity = c_str_to_string(identity_did);
        let batch_session = c_str_to_string(batch_session_id);
        let docs = match serde_json::from_str(&json_str) {
            Ok(JsonValue::Array(docs)) => docs,
            Ok(doc) => vec![doc],
            Err(e) => return FfiResult::error(format!("invalid JSON: {}", e)),
        };

        ffi_async!(caller, {
            let run = |mutation| crate::query::run_mutation(node_ptr, identity, batch_session, mutation);
            let ids = query::documents::add(run, &collection, &docs, &query::documents::AddOptions::default())
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_string(&ids).map_err(|e| e.to_string())
        })
    }
}
