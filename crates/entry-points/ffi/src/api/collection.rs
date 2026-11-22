//! Collections: add, delete, describe, patch, activate a version, truncate.

use std::ffi::{c_char, c_int};

use super::{collection, options, zero_default};
use crate::collection as calls;
use crate::types::DefraResult;

/// Options of `defra_collection_add`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraCollectionAddOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
}

/// Options of `defra_collection_delete`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraCollectionDeleteOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
    /// Delete only the active versions (1=true, 0=false).
    pub active_only: c_int,
    /// Reserved, so the struct has no padding: leave it zero. It never
    /// becomes a field; new fields go after the struct's last one.
    pub _reserved0: u32,
}

/// Options of `defra_collection_describe`. With no collection chosen,
/// every active collection is described.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraCollectionDescribeOptions {
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
    /// Reserved, so the struct has no padding: leave it zero. It never
    /// becomes a field; new fields go after the struct's last one.
    pub _reserved0: u32,
}

/// Options of `defra_collection_patch`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraCollectionPatchOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
    /// The migration for the versions the patch adds, a lens configuration
    /// (null or empty for none).
    pub lens_config: *const c_char,
}

/// Options of `defra_collection_set_active`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraCollectionSetActiveOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
}

/// Options of `defra_collection_truncate`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraCollectionTruncateOptions {
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
    /// Reserved, so the struct has no padding: leave it zero. It never
    /// becomes a field; new fields go after the struct's last one.
    pub _reserved0: u32,
    /// A GraphQL filter as a JSON object; null removes every document.
    pub filter_json: *const c_char,
}

zero_default!(
    DefraCollectionAddOptions,
    DefraCollectionDeleteOptions,
    DefraCollectionDescribeOptions,
    DefraCollectionPatchOptions,
    DefraCollectionSetActiveOptions,
    DefraCollectionTruncateOptions,
);

/// Adds the collections an SDL document defines. `defra_collection_add` in
/// `defra.h` passes the size.
///
/// # Safety
///
/// `sdl` must be null or a valid null-terminated string; `opts` must be null
/// or point to `opts_size` readable bytes whose strings are valid.
#[no_mangle]
pub unsafe extern "C" fn defra_collection_add_sized(
    target: usize,
    sdl: *const c_char,
    opts: *const DefraCollectionAddOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    calls::defra_collection_add(target, sdl, opts.identity)
}

/// Deletes the collections named in `names`, comma-separated.
///
/// # Safety
///
/// As [`defra_collection_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_collection_delete_sized(
    target: usize,
    names: *const c_char,
    opts: *const DefraCollectionDeleteOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    calls::defra_collection_delete(target, names, opts.active_only, opts.identity)
}

/// Describes the collection versions the options choose.
///
/// # Safety
///
/// As [`defra_collection_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_collection_describe_sized(
    target: usize,
    opts: *const DefraCollectionDescribeOptions,
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
    calls::defra_collection_describe(target, chosen, opts.identity)
}

/// Applies a JSON Patch to the collection definitions.
///
/// # Safety
///
/// As [`defra_collection_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_collection_patch_sized(
    target: usize,
    patch: *const c_char,
    opts: *const DefraCollectionPatchOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    calls::defra_collection_patch(target, patch, opts.lens_config, opts.identity)
}

/// Makes collection version `version_id` its collection's active one.
///
/// # Safety
///
/// As [`defra_collection_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_collection_set_active_sized(
    target: usize,
    version_id: *const c_char,
    opts: *const DefraCollectionSetActiveOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    let chosen = collection(std::ptr::null(), version_id, std::ptr::null(), 0, 0);
    calls::defra_collection_set_active(target, chosen, opts.identity)
}

/// Deletes the documents of the collection the options choose: every one,
/// or those `filter_json` matches.
///
/// # Safety
///
/// As [`defra_collection_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_collection_truncate_sized(
    target: usize,
    opts: *const DefraCollectionTruncateOptions,
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
    if opts.filter_json.is_null() {
        calls::defra_collection_truncate(target, chosen, opts.identity)
    } else {
        calls::defra_collection_truncate_with_filter(
            target,
            chosen,
            opts.identity,
            opts.filter_json,
        )
    }
}
