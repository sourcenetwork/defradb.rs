use std::ffi::c_char;

use acp::nac::NodePermission;

use super::select::collection;
use crate::request::Request;
use crate::target::{go_string, CollectionArgs};
use crate::types::{CollectionOptions, DefraResult};
use crate::{ffi_async_ok, ffi_entry, try_ffi};

/// Deletes every document in the collection `opts` selects, keeping the
/// collection. Every version of a collection shares its documents, so which
/// version `opts` selects does not change what is deleted.
///
/// # Safety
///
/// Every non-null string in `opts` must be a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_collection_truncate(
    node_ptr: usize,
    opts: CollectionOptions,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        truncate(node_ptr, &opts, identity_ptr, None)
    }
}

/// Deletes the documents of the collection `opts` selects that match
/// `filter_json`, a GraphQL filter as a JSON object, keeping the collection.
/// The documents are shared by every version of the collection; the filter
/// is checked against the active version's fields.
///
/// # Safety
///
/// `filter_json` must be null or a valid null-terminated string; every
/// non-null string in `opts` must be one too.
#[no_mangle]
pub unsafe extern "C" fn defra_collection_truncate_with_filter(
    node_ptr: usize,
    opts: CollectionOptions,
    identity_ptr: usize,
    filter_json: *const c_char,
) -> DefraResult {
    ffi_entry! {
        if filter_json.is_null() {
            return DefraResult::error("filter is required");
        }
        let filter: serde_json::Value = match serde_json::from_str(&go_string(filter_json)) {
            Ok(filter) => filter,
            Err(error) => return DefraResult::error(format!("invalid filter JSON: {}", error)),
        };
        // JSON null must not fall through to an unfiltered truncate.
        let conditions = match filter {
            serde_json::Value::Null => return DefraResult::error("filter cannot be null"),
            serde_json::Value::Object(conditions) => conditions,
            _ => return DefraResult::error("filter must be a JSON object"),
        };
        let filter = query::documents::Filter::from(serde_json::Value::Object(conditions));
        truncate(node_ptr, &opts, identity_ptr, Some(filter))
    }
}

/// # Safety
///
/// As for [`defra_collection_truncate`].
unsafe fn truncate(
    node_ptr: usize,
    opts: &CollectionOptions,
    identity_ptr: usize,
    filter: Option<query::documents::Filter>,
) -> DefraResult {
    let request = try_ffi!(Request::begin(
        node_ptr,
        identity_ptr,
        NodePermission::CollectionTruncate
    ));
    try_ffi!(request.require_node("collection truncate"));
    let selector = CollectionArgs::read(opts).selector();
    let runner = try_ffi!(request.runner());
    let context = request.context();
    ffi_async_ok!(caller, {
        let name = collection(&runner, &context, &selector).await?.name;
        runner
            .truncate_collection_as(&context, &name, filter.as_ref())
            .await
            .map_err(|e| e.to_string())
    })
}
