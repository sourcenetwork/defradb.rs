//! KV snapshot export and import (Go Shinzo's `ExportDocKVs`,
//! `ExportFieldMapping` and `ImportRawKVsWithMapping`), to and from files.

use std::ffi::c_char;

use acp::nac::NodePermission;
use db::kv_snapshot::{ExportOptions, FieldMapping};
use serde_json::json;

use crate::helpers::{get_node_database, require_c_str};
use crate::types::{c_str_to_string, FfiResult};
use crate::{ffi_async, ffi_entry, try_ffi};

/// Export documents of a collection the caller can read to `out_path` as a
/// KV snapshot in Go Shinzo's format, and return its field mapping.
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
/// * `identity_did` - Optional identity DID (nullable): the caller the export reads as
/// * `collection_name` - The collection to export
/// * `doc_ids_json` - Optional (nullable) JSON array of DocIDs; null or `[]` exports every document
/// * `options_json` - Optional (nullable) `{"datastoreOnly": bool, "history": bool}`
///   (history defaults to true: earlier versions and signatures are included)
/// * `out_path` - The file to write the snapshot to
///
/// # Returns
///
/// - Status 0: Success (value is `{"frames": N, "mapping": {...}}`; the
///   mapping is the JSON an importing node takes, Go's shape)
/// - Status 1: Error (error field contains message)
///
/// # Safety
///
/// `collection_name` and `out_path` must be valid null-terminated UTF-8
/// strings; the nullable arguments null or such strings.
#[no_mangle]
pub unsafe extern "C" fn kv_export(
    node_ptr: usize,
    identity_did: *const c_char,
    collection_name: *const c_char,
    doc_ids_json: *const c_char,
    options_json: *const c_char,
    out_path: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let collection = try_ffi!(require_c_str(collection_name, "collection_name"));
        let out_path = try_ffi!(require_c_str(out_path, "out_path"));
        let doc_ids: Vec<String> = match c_str_to_string(doc_ids_json).filter(|json| !json.is_empty()) {
            Some(json) => try_ffi!(serde_json::from_str(&json)
                .map_err(|e| FfiResult::error(format!("invalid doc_ids_json: {e}")))),
            None => Vec::new(),
        };
        let options = try_ffi!(export_options(c_str_to_string(options_json)));
        let identity = c_str_to_string(identity_did).filter(|did| !did.is_empty());
        let database = try_ffi!(get_node_database(node_ptr));

        ffi_async!(node_ptr, {
            let context = crate::query::caller_context(
                node_ptr,
                identity,
                None,
                NodePermission::DocumentRead,
            )
            .await?;
            let mut snapshot = Vec::new();
            let exported = database
                .export_kv_snapshot_as(&context, &collection, &doc_ids, options, &mut snapshot)
                .await
                .map_err(|e| e.to_string())?;
            std::fs::write(&out_path, &snapshot)
                .map_err(|e| format!("failed to write '{out_path}': {e}"))?;
            Ok(json!({ "frames": exported.frames, "mapping": exported.mapping }).to_string())
        })
    }
}

/// Import the KV snapshot at `in_path`, written by this node, another
/// defradb.rs node or Go Shinzo, with the source's field mapping. Each
/// document is replayed through the merge as a restore, so the replication
/// filter and the retention floor do not judge it.
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
/// * `identity_did` - Optional identity DID (nullable): the caller the import runs as
/// * `in_path` - The snapshot file
/// * `mapping_json` - Optional (nullable) field mapping the export returned
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

fn export_options(json: Option<String>) -> Result<ExportOptions, FfiResult> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", default)]
    struct Options {
        datastore_only: bool,
        history: bool,
    }
    impl Default for Options {
        fn default() -> Self {
            let defaults = ExportOptions::default();
            Self {
                datastore_only: defaults.datastore_only,
                history: defaults.history,
            }
        }
    }
    let options: Options = match json.filter(|json| !json.is_empty()) {
        Some(json) => serde_json::from_str(&json)
            .map_err(|e| FfiResult::error(format!("invalid options_json: {e}")))?,
        None => Options::default(),
    };
    Ok(ExportOptions {
        datastore_only: options.datastore_only,
        history: options.history,
    })
}

/// The field mapping an importing node reads a KV snapshot of
/// `collection_name` with (Go Shinzo's `ExportFieldMapping`), without
/// exporting anything.
///
/// # Returns
///
/// - Status 0: Success (value is the mapping JSON, Go's shape)
/// - Status 1: Error (error field contains message)
///
/// # Safety
///
/// `collection_name` must be a valid null-terminated UTF-8 string;
/// `identity_did` null or such a string.
#[no_mangle]
pub unsafe extern "C" fn kv_field_mapping(
    node_ptr: usize,
    identity_did: *const c_char,
    collection_name: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let collection = try_ffi!(require_c_str(collection_name, "collection_name"));
        let identity = c_str_to_string(identity_did).filter(|did| !did.is_empty());
        let database = try_ffi!(get_node_database(node_ptr));

        ffi_async!(node_ptr, {
            crate::query::caller_context(node_ptr, identity, None, NodePermission::CollectionGet)
                .await?;
            let mapping = database.kv_field_mapping(&collection).map_err(|e| e.to_string())?;
            serde_json::to_string(&mapping).map_err(|e| e.to_string())
        })
    }
}
