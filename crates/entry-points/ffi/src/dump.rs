use crate::request::Request;
use crate::types::DefraResult;
use crate::{ffi_async_ok, ffi_entry, try_ffi};

/// Logs every key of the node's store, with its value's size. Unwise on a
/// large database. The caller is `identity_ptr`; as in Go, the dump checks
/// no permission.
pub(crate) fn defra_print_dump(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::resolve(node_ptr, identity_ptr));
        let node = try_ffi!(request.require_node("print dump"));
        let database = try_ffi!(request.database());
        ffi_async_ok!(node, {
            for line in database.print_dump().await.map_err(|e| e.to_string())? {
                tracing::info!(target: "defra::dump", "{}", line);
            }
            Ok(())
        })
    }
}
