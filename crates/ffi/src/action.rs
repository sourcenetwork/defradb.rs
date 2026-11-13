use acp::nac::NodePermission;

use crate::request::Request;
use crate::types::DefraResult;
use crate::{ffi_async, ffi_entry, try_ffi};

/// List actions that are in progress or ended with an error.
///
#[no_mangle]
pub extern "C" fn defra_action_list(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::ActionList));
        let node = try_ffi!(request.require_node("action list"));
        let database = try_ffi!(request.database());
        let context = request.context();
        ffi_async!(node, {
            let actions = database
                .list_actions_as(&context)
                .await
                .map_err(|e| format!("failed to list actions: {}", e))?;
            serde_json::to_string(&actions)
                .map_err(|e| format!("failed to serialize actions: {}", e))
        })
    }
}
