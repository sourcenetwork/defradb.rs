use std::ffi::{c_char, c_int};

use acp::nac::NodePermission;

use crate::request::Request;
use crate::target::{go_string, split_comma_separated};
use crate::types::DefraResult;
use crate::{ffi_async_ok, ffi_entry, try_ffi};

/// Deletes the comma-separated collections `names`, on a node or in a
/// transaction; with `active_only` set, only their active versions.
///
/// # Safety
///
/// `names` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_collection_delete(
    node_ptr: usize,
    names: *const c_char,
    active_only: c_int,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::CollectionPatch));
        let names = split_comma_separated(&go_string(names));
        let active_only = active_only != 0;
        let context = request.context();
        match request.target.txn_id() {
            None => {
                let database = try_ffi!(request.database());
                ffi_async_ok!(request.node(), {
                    database
                        .delete_collections_as(&context, names, active_only)
                        .await
                        .map_err(|e| e.to_string())
                })
            }
            Some(_) => {
                let runner = try_ffi!(request.runner());
                ffi_async_ok!(caller, {
                    runner
                        .delete_collections_as(&context, names, active_only)
                        .await
                        .map_err(|e| e.to_string())
                })
            }
        }
    }
}
