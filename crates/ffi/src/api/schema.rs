//! Views, lenses, backups and the store dump.

use std::ffi::{c_char, c_int};

use super::{collection, options, zero_default};
use crate::types::DefraResult;
use crate::{backup, dump, lens, view};

/// Options of `defra_view_add`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraViewAddOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
    /// The CID of a lens to apply to the view's rows (null or empty for none).
    pub transform_cid: *const c_char,
}

/// Options of `defra_view_refresh`. With no collection chosen, every view is
/// refreshed.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraViewRefreshOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
    /// Collection name.
    pub collection_name: *const c_char,
    /// Collection version ID.
    pub collection_version_id: *const c_char,
    /// Collection ID.
    pub collection_id: *const c_char,
    /// Include inactive collection versions (1=true, 0=false).
    pub collection_get_inactive: c_int,
}

/// Options of the three lens calls.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraLensOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
}

/// Options of `defra_backup_export`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraBackupExportOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
    /// Comma-separated collections to export (null or empty for every one).
    pub collections: *const c_char,
    /// The format (null or empty for "json", the only one).
    pub format: *const c_char,
    /// Indent the JSON (1=true, 0=false).
    pub pretty: c_int,
}

/// Options of `defra_backup_import`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraBackupImportOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
}

/// Options of `defra_print_dump`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraPrintDumpOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
}

zero_default!(
    DefraViewAddOptions,
    DefraViewRefreshOptions,
    DefraLensOptions,
    DefraBackupExportOptions,
    DefraBackupImportOptions,
    DefraPrintDumpOptions,
);

/// Adds a view: the collection `sdl` defines, whose documents `query`
/// answers.
///
/// # Safety
///
/// Every string must be null or a valid null-terminated string; `opts` must
/// be null or point to `opts_size` readable bytes whose strings are valid.
#[no_mangle]
pub unsafe extern "C" fn defra_view_add_sized(
    target: usize,
    query: *const c_char,
    sdl: *const c_char,
    opts: *const DefraViewAddOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    view::defra_view_add(target, query, sdl, opts.transform_cid, opts.identity)
}

/// Refreshes the materialized views the options choose.
///
/// # Safety
///
/// As [`defra_view_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_view_refresh_sized(
    target: usize,
    opts: *const DefraViewRefreshOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    let chosen = collection(
        opts.collection_name,
        opts.collection_version_id,
        opts.collection_id,
        opts.collection_get_inactive,
        0,
    );
    view::defra_view_refresh(target, chosen, opts.identity)
}

/// Adds the lens `config` describes, and answers with its CID.
///
/// # Safety
///
/// As [`defra_view_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_lens_add_sized(
    target: usize,
    config: *const c_char,
    opts: *const DefraLensOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    lens::defra_lens_add(target, opts.identity, config)
}

/// Lists the lenses.
///
/// # Safety
///
/// As [`defra_view_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_lens_list_sized(
    target: usize,
    opts: *const DefraLensOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    lens::defra_lens_list(target, opts.identity)
}

/// Sets the migration from collection version `src` to `dst` to the lens
/// `config` describes.
///
/// # Safety
///
/// As [`defra_view_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_lens_set_sized(
    target: usize,
    src: *const c_char,
    dst: *const c_char,
    config: *const c_char,
    opts: *const DefraLensOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    lens::defra_lens_set(target, opts.identity, src, dst, config)
}

/// Exports the documents the caller can read to the JSON file `filepath`.
///
/// # Safety
///
/// As [`defra_view_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_backup_export_sized(
    target: usize,
    filepath: *const c_char,
    opts: *const DefraBackupExportOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    backup::defra_backup_export(
        target,
        filepath,
        opts.collections,
        opts.format,
        opts.pretty,
        opts.identity,
    )
}

/// Imports the documents in the JSON backup file `filepath`.
///
/// # Safety
///
/// As [`defra_view_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_backup_import_sized(
    target: usize,
    filepath: *const c_char,
    opts: *const DefraBackupImportOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    backup::defra_backup_import(target, filepath, opts.identity)
}

/// Logs every key of the node's store, with its value's size. Unwise on a
/// large database.
///
/// # Safety
///
/// `opts` must be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_print_dump_sized(
    target: usize,
    opts: *const DefraPrintDumpOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    dump::defra_print_dump(target, opts.identity)
}
