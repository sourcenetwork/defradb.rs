//! Secondary and encrypted indexes.

use std::ffi::{c_char, c_int};

use super::{collection, options, zero_default};
use crate::types::DefraResult;
use crate::{encrypted_index, index};

/// Options of `defra_index_new`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraIndexNewOptions {
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
    pub reserved0: u32,
    /// The index name (null or empty to generate one).
    pub name: *const c_char,
    /// A unique index (1=true, 0=false).
    pub unique: c_int,
    /// Reserved, so the struct has no padding: leave it zero. It never
    /// becomes a field; new fields go after the struct's last one.
    pub reserved1: u32,
    /// A vector index description as JSON (null or empty for an ordered index).
    pub vector_json: *const c_char,
}

/// Options of `defra_index_delete`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraIndexDeleteOptions {
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
    pub reserved: u32,
}

/// Options of `defra_index_list`. With no collection chosen, every
/// collection's indexes are listed.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraIndexListOptions {
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
    pub reserved: u32,
}

/// Options of `defra_encrypted_index_new`. Encrypted indexes choose their
/// collection by name.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraEncryptedIndexNewOptions {
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
    pub reserved: u32,
    /// The index type (null or empty for "equality").
    pub index_type: *const c_char,
}

/// Options of `defra_encrypted_index_delete`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraEncryptedIndexDeleteOptions {
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
    pub reserved: u32,
}

/// Options of `defra_encrypted_index_list`. With no collection named, every
/// collection's encrypted indexes are listed.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraEncryptedIndexListOptions {
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
    pub reserved: u32,
}

zero_default!(
    DefraIndexNewOptions,
    DefraIndexDeleteOptions,
    DefraIndexListOptions,
    DefraEncryptedIndexNewOptions,
    DefraEncryptedIndexDeleteOptions,
    DefraEncryptedIndexListOptions,
);

/// Creates an index over `fields`: comma-separated field names, each
/// optionally `:ASC` (the default) or `:DESC`.
///
/// # Safety
///
/// `fields` must be null or a valid null-terminated string; `opts` must be
/// null or point to `opts_size` readable bytes whose strings are valid.
#[no_mangle]
pub unsafe extern "C" fn defra_index_new_sized(
    target: usize,
    fields: *const c_char,
    opts: *const DefraIndexNewOptions,
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
    index::defra_index_new(
        target,
        opts.name,
        fields,
        opts.unique,
        opts.vector_json,
        chosen,
        opts.identity,
    )
}

/// Deletes index `name`.
///
/// # Safety
///
/// As [`defra_index_new_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_index_delete_sized(
    target: usize,
    name: *const c_char,
    opts: *const DefraIndexDeleteOptions,
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
    index::defra_index_delete(target, name, chosen, opts.identity)
}

/// Lists the indexes of the collection the options choose, or of every
/// collection.
///
/// # Safety
///
/// As [`defra_index_new_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_index_list_sized(
    target: usize,
    opts: *const DefraIndexListOptions,
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
    index::defra_index_list(target, chosen, opts.identity)
}

/// Creates an encrypted index on `field`.
///
/// # Safety
///
/// As [`defra_index_new_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_encrypted_index_new_sized(
    target: usize,
    field: *const c_char,
    opts: *const DefraEncryptedIndexNewOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    encrypted_index::defra_encrypted_index_new(
        target,
        opts.collection_name,
        field,
        opts.index_type,
        opts.identity,
    )
}

/// Deletes the encrypted index on `field`.
///
/// # Safety
///
/// As [`defra_index_new_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_encrypted_index_delete_sized(
    target: usize,
    field: *const c_char,
    opts: *const DefraEncryptedIndexDeleteOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    encrypted_index::defra_encrypted_index_delete(
        target,
        opts.collection_name,
        field,
        opts.identity,
    )
}

/// Lists the encrypted indexes of the collection named, or of every
/// collection.
///
/// # Safety
///
/// As [`defra_index_new_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_encrypted_index_list_sized(
    target: usize,
    opts: *const DefraEncryptedIndexListOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    encrypted_index::defra_encrypted_index_list(target, opts.collection_name, opts.identity)
}
