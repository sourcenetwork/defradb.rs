use acp::nac::NodePermission;

use super::select::collections;
use crate::request::Request;
use crate::target::CollectionArgs;
use crate::types::{CollectionOptions, DefraResult};
use crate::{ffi_async, ffi_entry, try_ffi};

/// Answers with the collection versions `opts` selects as JSON, on a node or
/// in a transaction.
///
/// # Safety
///
/// Every non-null string in `opts` must be a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_collection_describe(
    node_ptr: usize,
    opts: CollectionOptions,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::CollectionGet));
        let selector = CollectionArgs::read(&opts).selector();
        let runner = try_ffi!(request.runner());
        let context = request.context();
        ffi_async!(caller, {
            let selected = collections(&runner, &context, &selector).await?;
            serde_json::to_string(&selected).map_err(|e| format!("failed to serialize result: {}", e))
        })
    }
}
