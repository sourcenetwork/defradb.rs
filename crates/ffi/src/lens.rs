//! Lenses: WASM transforms, and the migrations between collection versions
//! they make.

use std::ffi::c_char;

use acp::nac::NodePermission;
use lens::LensConfig;

use crate::request::Request;
use crate::target::go_string;
use crate::types::DefraResult;
use crate::{ffi_async, ffi_entry, try_ffi};

/// A lens configuration, Go's `model.Lens`: `{"Lenses": [...]}`.
fn parse_lens(config: &str) -> Result<LensConfig, String> {
    serde_json::from_str(config).map_err(|e| format!("failed to parse lens config: {}", e))
}

/// Adds a lens, on a node or in a transaction, and answers with its ID.
///
/// # Safety
///
/// `cfg` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_lens_add(
    node_ptr: usize,
    identity_ptr: usize,
    cfg: *const c_char,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::LensCreate));
        let cfg = go_string(cfg);
        let context = request.context();
        let runner = try_ffi!(request.runner());
        ffi_async!(caller, {
            let config = parse_lens(&cfg)?;
            runner
                .add_lens_as(&context, config)
                .await
                .map_err(|e| format!("failed to add lens: {}", e))
        })
    }
}

/// Answers with the node's lenses as JSON, on a node or in a transaction.
#[no_mangle]
pub extern "C" fn defra_lens_list(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::LensList));
        let context = request.context();
        let runner = try_ffi!(request.runner());
        ffi_async!(caller, {
            runner
                .list_lenses_as(&context)
                .await
                .map_err(|e| format!("failed to list lenses: {}", e))
        })
    }
}

/// Sets the lens `cfg` as the migration from collection version `src` to
/// `dst`, on a node or in a transaction, and answers with the lens ID.
///
/// # Safety
///
/// `src`, `dst` and `cfg` must be null or valid null-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn defra_lens_set(
    node_ptr: usize,
    identity_ptr: usize,
    src: *const c_char,
    dst: *const c_char,
    cfg: *const c_char,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::MigrationSet));
        let (src, dst, cfg) = (go_string(src), go_string(dst), go_string(cfg));
        let context = request.context();
        let migration = move || {
            let mut config = parse_lens(&cfg)?;
            config.source_collection_version_id = src;
            config.destination_collection_version_id = dst;
            Ok::<_, String>(config)
        };
        let runner = try_ffi!(request.runner());
        ffi_async!(caller, {
            let migration = serde_json::to_string(&migration()?)
                .map_err(|e| format!("failed to serialize lens config: {}", e))?;
            runner
                .set_migration_as(&context, &migration)
                .await
                .map_err(|e| format!("failed to set migration: {}", e))
        })
    }
}
