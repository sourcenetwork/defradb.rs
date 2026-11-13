use std::ffi::{c_char, c_int};
use std::fs;

use crate::request::Request;
use crate::state::NODES;
use crate::target::{go_string, split_comma_separated};
use crate::types::DefraResult;
use crate::{ffi_async_ok, ffi_entry, try_ffi, ERR_INVALID_NODE_HANDLE};

/// Exports the documents of the comma-separated `collections` (every
/// collection for null or empty) to the file `filepath`, as JSON (`format`
/// "json", the default for null or empty), indented when `pretty` is set.
///
/// # Safety
///
/// Every string must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_backup_export(
    node_ptr: usize,
    filepath: *const c_char,
    collections: *const c_char,
    format: *const c_char,
    pretty: c_int,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::resolve(node_ptr, 0));
        let node_ptr = try_ffi!(request.require_node("backup export"));
        match go_string(format).to_lowercase().as_str() {
            "" | "json" => {}
            other => return DefraResult::error(format!("unsupported export format: {}", other)),
        }
        let filepath = go_string(filepath);
        let collections = split_comma_separated(&go_string(collections));
        let pretty = pretty != 0;

        let (database, runner) = try_ffi!(NODES
            .get(node_ptr, |state| (state.database.clone(), state.query_runner.clone()))
            .ok_or_else(|| DefraResult::error(ERR_INVALID_NODE_HANDLE)));

        ffi_async_ok!(node_ptr, {
            let json_output =
                db::backup::export_database(&database, &runner, &collections, pretty).await?;

            // Written to a temporary file and renamed, so a failed export
            // never leaves a partial file at `filepath`.
            let temp_path = format!("{}.temp", filepath);
            fs::write(&temp_path, &json_output)
                .map_err(|e| format!("failed to create file '{}': {}", temp_path, e))?;
            fs::rename(&temp_path, &filepath).map_err(|e| {
                let _ = fs::remove_file(&temp_path);
                format!("failed to rename temp file: {}", e)
            })
        })
    }
}
