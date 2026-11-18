use std::ffi::c_char;
use std::fs;

use crate::ffi_entry;
use crate::request::Request;
use crate::state::NODES;
use crate::target::go_string;
use crate::types::DefraResult;
use crate::{ffi_async_ok, try_ffi, ERR_INVALID_NODE_HANDLE};

/// Import documents from a JSON backup file.
///
/// The file must be a JSON object mapping collection names to arrays of documents:
/// ```json
/// {
///     "User": [{"_docID": "...", "_docIDNew": "...", "name": "John", "age": 30}],
///     "Address": [{"_docID": "...", "_docIDNew": "...", "street": "...", "city": "..."}]
/// }
/// ```
///
/// Self-referencing FK fields are stripped before creation and applied
/// via update afterward, matching Go DefraDB behavior.
///
/// # Safety
///
/// `filepath` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_backup_import(
    node_ptr: usize,
    filepath: *const c_char,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::resolve(node_ptr, 0));
        let node_ptr = try_ffi!(request.require_node("backup import"));
        let path_str = go_string(filepath);

        let (database, runner) = match NODES.get(node_ptr, |state| {
            (state.database.clone(), state.query_runner.clone())
        }) {
            Some(r) => r,
            None => return DefraResult::error(ERR_INVALID_NODE_HANDLE),
        };

        ffi_async_ok!(node_ptr, {
            let content = fs::read_to_string(&path_str).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    format!("failed to open file '{}': {}", path_str, e)
                } else {
                    format!("failed to read file '{}': {}", path_str, e)
                }
            })?;

            db::backup::import_database(&database, &runner, &content)
                .await
                .map(|_| ())
        })
    }
}
