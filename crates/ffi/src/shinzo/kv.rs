//! KV snapshot import (Go Shinzo's `ImportRawKVsWithMapping`), from a file
//! an old Go Shinzo node exported.

use std::ffi::c_char;

use acp::nac::NodePermission;
use db::kv_snapshot::FieldMapping;
use serde_json::json;

use crate::helpers::{get_node_database, require_c_str};
use crate::types::{c_str_to_string, FfiResult};
use crate::{ffi_async, ffi_entry, try_ffi};

/// Import the KV snapshot at `in_path`, exported by Go Shinzo, with the
/// source's field mapping. Each document is replayed through the merge as a
/// restore, so the replication filter and the retention floor do not judge
/// it.
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
/// * `identity_did` - Optional identity DID (nullable): the caller the import runs as
/// * `in_path` - The snapshot file
/// * `mapping_json` - Optional (nullable) field mapping Go's `ExportFieldMapping` wrote
///
/// # Returns
///
/// - Status 0: Success (value is `{"frames": N, "documents": M}`)
/// - Status 1: Error (error field contains message), including a document
///   whose history the snapshot does not carry back to its first version,
///   after every other document is imported
///
/// # Safety
///
/// `in_path` must be a valid null-terminated UTF-8 string; the nullable
/// arguments null or such strings.
#[no_mangle]
pub unsafe extern "C" fn kv_import(
    node_ptr: usize,
    identity_did: *const c_char,
    in_path: *const c_char,
    mapping_json: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let in_path = try_ffi!(require_c_str(in_path, "in_path"));
        let mapping: Option<FieldMapping> = match c_str_to_string(mapping_json).filter(|json| !json.is_empty()) {
            Some(json) => Some(try_ffi!(serde_json::from_str(&json)
                .map_err(|e| FfiResult::error(format!("invalid mapping_json: {e}"))))),
            None => None,
        };
        let identity = c_str_to_string(identity_did).filter(|did| !did.is_empty());
        let database = try_ffi!(get_node_database(node_ptr));

        ffi_async!(node_ptr, {
            let context = crate::query::caller_context(
                node_ptr,
                identity,
                None,
                NodePermission::DocumentUpdate,
            )
            .await?;
            let snapshot = std::fs::read(&in_path)
                .map_err(|e| format!("failed to read '{in_path}': {e}"))?;
            let imported = database
                .import_kv_snapshot_as(&context, &snapshot[..], mapping.as_ref())
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({ "frames": imported.frames, "documents": imported.documents }).to_string())
        })
    }
}
