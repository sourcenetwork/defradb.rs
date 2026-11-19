use std::ffi::c_char;

use acp::nac::NodePermission;

use crate::request::Request;
use crate::target::go_string;
use crate::types::DefraResult;
use crate::{ffi_async, ffi_entry, try_ffi};

/// Adds the collections an SDL document defines, on a node or in a
/// transaction, and answers with the created collection versions as JSON.
///
/// # Safety
///
/// `sdl` must be null or a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn defra_collection_add(
    node_ptr: usize,
    sdl: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::CollectionPatch));
        let sdl = go_string(sdl);
        let context = request.context();
        match request.target.txn_id() {
            None => {
                let database = try_ffi!(request.database());
                ffi_async!(request.node(), {
                    let created = database
                        .add_collection(&context, &sdl)
                        .await
                        .map_err(|e| e.to_string())?;
                    serde_json::to_string(&created)
                        .map_err(|e| format!("failed to serialize result: {}", e))
                })
            }
            Some(_) => {
                let runner = try_ffi!(request.runner());
                ffi_async!(caller, {
                    let created = runner
                        .add_collection_as(&context, &sdl)
                        .await
                        .map_err(|e| e.to_string())?;
                    serde_json::to_string(&created)
                        .map_err(|e| format!("failed to serialize result: {}", e))
                })
            }
        }
    }
}
