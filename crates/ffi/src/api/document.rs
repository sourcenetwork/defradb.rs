//! Documents: add, save many, get, update, delete, purge.

use std::ffi::{c_char, c_int};

use super::{collection, options, zero_default};
use crate::document as calls;
use crate::types::DefraResult;

/// Options of `defra_document_add` and `defra_document_save_many`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraDocumentAddOptions {
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
    /// Encrypt every field (1=true, 0=false).
    pub encrypt: c_int,
    /// Comma-separated fields to encrypt (null or empty for none).
    pub encrypted_fields: *const c_char,
    /// Block signing: 0 node default, 1 sign, -1 do not sign.
    pub signing: c_int,
}

/// Options of `defra_document_get`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraDocumentGetOptions {
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
    /// Answer a deleted document too (1=true, 0=false).
    pub show_deleted: c_int,
}

/// Options of `defra_document_update`: the documents are `doc_id`, or those
/// `filter_json` matches.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraDocumentUpdateOptions {
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
    /// The document to update.
    pub doc_id: *const c_char,
    /// A GraphQL filter choosing the documents to update, as JSON.
    pub filter_json: *const c_char,
    /// Block signing: 0 node default, 1 sign, -1 do not sign.
    pub signing: c_int,
}

/// Options of `defra_document_delete`: the documents are `doc_id`, or those
/// `filter_json` matches.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraDocumentDeleteOptions {
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
    /// The document to delete.
    pub doc_id: *const c_char,
    /// A GraphQL filter choosing the documents to delete, as JSON.
    pub filter_json: *const c_char,
    /// Block signing: 0 node default, 1 sign, -1 do not sign.
    pub signing: c_int,
}

/// Options of `defra_document_purge`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraDocumentPurgeOptions {
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

zero_default!(
    DefraDocumentAddOptions,
    DefraDocumentGetOptions,
    DefraDocumentUpdateOptions,
    DefraDocumentDeleteOptions,
    DefraDocumentPurgeOptions,
);

/// Adds the document, or the JSON array of documents, in `json`.
///
/// # Safety
///
/// `json` must be null or a valid null-terminated string; `opts` must be
/// null or point to `opts_size` readable bytes whose strings are valid.
#[no_mangle]
pub unsafe extern "C" fn defra_document_add_sized(
    target: usize,
    json: *const c_char,
    opts: *const DefraDocumentAddOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    let chosen = collection(
        opts.collection_name,
        opts.collection_version_id,
        opts.collection_id,
        opts.collection_get_inactive,
        opts.signing,
    );
    calls::defra_document_add(
        target,
        json,
        opts.encrypt,
        opts.encrypted_fields,
        chosen,
        opts.identity,
    )
}

/// Saves each document in the JSON array `json`: added if new, updated if
/// it exists.
///
/// # Safety
///
/// As [`defra_document_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_document_save_many_sized(
    target: usize,
    json: *const c_char,
    opts: *const DefraDocumentAddOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    let chosen = collection(
        opts.collection_name,
        opts.collection_version_id,
        opts.collection_id,
        opts.collection_get_inactive,
        opts.signing,
    );
    calls::defra_document_save_many(
        target,
        json,
        opts.encrypt,
        opts.encrypted_fields,
        chosen,
        opts.identity,
    )
}

/// Answers document `doc_id`.
///
/// # Safety
///
/// As [`defra_document_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_document_get_sized(
    target: usize,
    doc_id: *const c_char,
    opts: *const DefraDocumentGetOptions,
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
    calls::defra_document_get(target, doc_id, opts.show_deleted, chosen, opts.identity)
}

/// Applies the JSON `updater` to the documents the options choose.
///
/// # Safety
///
/// As [`defra_document_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_document_update_sized(
    target: usize,
    updater: *const c_char,
    opts: *const DefraDocumentUpdateOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    let chosen = collection(
        opts.collection_name,
        opts.collection_version_id,
        opts.collection_id,
        opts.collection_get_inactive,
        opts.signing,
    );
    calls::defra_document_update(
        target,
        opts.doc_id,
        opts.filter_json,
        updater,
        chosen,
        opts.identity,
    )
}

/// Deletes the documents the options choose.
///
/// # Safety
///
/// As [`defra_document_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_document_delete_sized(
    target: usize,
    opts: *const DefraDocumentDeleteOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    let chosen = collection(
        opts.collection_name,
        opts.collection_version_id,
        opts.collection_id,
        opts.collection_get_inactive,
        opts.signing,
    );
    calls::defra_document_delete(target, opts.doc_id, opts.filter_json, chosen, opts.identity)
}

/// Removes the documents in `doc_ids`, comma-separated, and their history
/// from this node.
///
/// # Safety
///
/// As [`defra_document_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_document_purge_sized(
    target: usize,
    doc_ids: *const c_char,
    opts: *const DefraDocumentPurgeOptions,
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
    calls::defra_document_purge(target, doc_ids, chosen, opts.identity)
}
