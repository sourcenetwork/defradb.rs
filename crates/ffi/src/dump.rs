//! Go's `PrintDump`: every key of the store, for debugging.

use std::ffi::c_char;

use acp::nac::NodePermission;

use crate::helpers::get_node_database;
use crate::nac_check::check_nac_for_node;
use crate::types::FfiResult;
use crate::{ffi_async, ffi_entry, try_ffi};

/// Every key/value pair of the store as a human-readable line, Go's
/// `PrintDump`.
///
/// # Returns
///
/// - Status 0: Success (value is a JSON array of strings)
/// - Status 1: Error (error field contains message)
///
/// # Safety
///
/// `identity_did` must be null or a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn print_dump(node_ptr: usize, identity_did: *const c_char) -> FfiResult {
    ffi_entry! {
        try_ffi!(check_nac_for_node(node_ptr, identity_did, NodePermission::DocumentRead));
        let database = try_ffi!(get_node_database(node_ptr));
        ffi_async!(node_ptr, {
            let lines = database.print_dump().await?;
            serde_json::to_string(&lines).map_err(|e| format!("failed to serialize dump: {}", e))
        })
    }
}
