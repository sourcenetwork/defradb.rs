//! P2P: peers, the collections and documents a node replicates, syncing,
//! replicators, and the mobile helpers.

use std::ffi::c_char;

use super::{c_ptr, options, timeout, zero_default};
use crate::p2p;
use crate::types::DefraResult;

/// Options of every P2P and mobile call that needs nothing but the caller.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraP2POptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
}

/// Options of the three sync calls.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraP2PSyncOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
    /// How long the sync may take, in milliseconds (0 for no limit). A value
    /// beyond about 292 years, `i64::MAX` included, is that longest timeout.
    pub timeout_ms: i64,
}

/// Options of `defra_p2p_replicator_add`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraReplicatorAddOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
    /// Replication filters as JSON (null or empty to replicate every
    /// document).
    pub filters_json: *const c_char,
}

/// Options of `defra_p2p_replicator_delete`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraReplicatorDeleteOptions {
    /// The caller's identity (0 for none).
    pub identity: usize,
    /// Comma-separated collections to stop replicating (null or empty for
    /// every one).
    pub collections: *const c_char,
}

zero_default!(
    DefraP2POptions,
    DefraP2PSyncOptions,
    DefraReplicatorAddOptions,
    DefraReplicatorDeleteOptions,
);

/// Connects to the peers at `peer_addresses`, comma-separated.
///
/// # Safety
///
/// `peer_addresses` must be null or a valid null-terminated string; `opts` must be
/// null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_connect_sized(
    target: usize,
    peer_addresses: *const c_char,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_p2p_connect(target, peer_addresses, opts.identity)
}

/// Disconnects from the peers at `peer_addresses`, comma-separated.
///
/// # Safety
///
/// `peer_addresses` must be null or a valid null-terminated string; `opts` must be
/// null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_disconnect_sized(
    target: usize,
    peer_addresses: *const c_char,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_p2p_disconnect(target, peer_addresses, opts.identity)
}

/// Answers this node's peer information.
///
/// # Safety
///
/// `opts` must be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_info_sized(
    target: usize,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_p2p_info(target, opts.identity)
}

/// Lists the connected peers.
///
/// # Safety
///
/// `opts` must be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_peer_list_sized(
    target: usize,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_p2p_peer_list(target, opts.identity)
}

/// Starts replicating the collections in `collections`, comma-separated.
///
/// # Safety
///
/// `collections` must be null or a valid null-terminated string; `opts` must be
/// null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_collection_add_sized(
    target: usize,
    collections: *const c_char,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_p2p_collection_add(target, collections, opts.identity)
}

/// Stops replicating the collections in `collections`, comma-separated.
///
/// # Safety
///
/// `collections` must be null or a valid null-terminated string; `opts` must be
/// null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_collection_delete_sized(
    target: usize,
    collections: *const c_char,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_p2p_collection_delete(target, collections, opts.identity)
}

/// Lists the collections this node replicates.
///
/// # Safety
///
/// `opts` must be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_collection_list_sized(
    target: usize,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_p2p_collection_list(target, opts.identity)
}

/// Starts replicating the documents in `doc_ids`, comma-separated.
///
/// # Safety
///
/// `doc_ids` must be null or a valid null-terminated string; `opts` must be
/// null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_document_add_sized(
    target: usize,
    doc_ids: *const c_char,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_p2p_document_add(target, doc_ids, opts.identity)
}

/// Stops replicating the documents in `doc_ids`, comma-separated.
///
/// # Safety
///
/// `doc_ids` must be null or a valid null-terminated string; `opts` must be
/// null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_document_delete_sized(
    target: usize,
    doc_ids: *const c_char,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_p2p_document_delete(target, doc_ids, opts.identity)
}

/// Lists the documents this node replicates.
///
/// # Safety
///
/// `opts` must be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_document_list_sized(
    target: usize,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_p2p_document_list(target, opts.identity)
}

/// Retries every replicator that failed to receive updates.
///
/// # Safety
///
/// `opts` must be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_replicator_retry_sized(
    target: usize,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_p2p_replicator_retry(target, opts.identity)
}

/// Lists the replicators.
///
/// # Safety
///
/// `opts` must be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_replicator_list_sized(
    target: usize,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_p2p_replicator_list(target, opts.identity)
}

/// Answers an address another device can connect to this node at.
///
/// # Safety
///
/// `opts` must be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_mobile_shareable_address_sized(
    target: usize,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_mobile_shareable_address(target, opts.identity)
}

/// Answers how far this node's sync has got.
///
/// # Safety
///
/// `opts` must be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_mobile_sync_status_sized(
    target: usize,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_mobile_sync_status(target, opts.identity)
}

/// Tells the node its network changed, so it reconnects.
///
/// # Safety
///
/// `opts` must be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_mobile_notify_network_change_sized(
    target: usize,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_mobile_notify_network_change(target, opts.identity)
}

/// Answers what this node knows of peer `peer_id`.
///
/// # Safety
///
/// `peer_id` must be null or a valid null-terminated string; `opts` must be
/// null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_mobile_peer_details_sized(
    target: usize,
    peer_id: *const c_char,
    opts: *const DefraP2POptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_mobile_peer_details(target, peer_id, opts.identity)
}

/// Syncs branchable collection `collection_id` from connected peers.
///
/// # Safety
///
/// `collection_id` must be null or a valid null-terminated string; `opts`
/// must be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_collection_sync_branchable_sized(
    target: usize,
    collection_id: *const c_char,
    opts: *const DefraP2PSyncOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    let limit = match timeout(opts.timeout_ms) {
        Ok(limit) => limit,
        Err(error) => return error,
    };
    p2p::defra_p2p_collection_sync_branchable(target, collection_id, c_ptr(&limit), opts.identity)
}

/// Syncs the collection versions in `version_ids`, comma-separated, from
/// connected peers.
///
/// # Safety
///
/// As [`defra_p2p_collection_sync_branchable_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_collection_sync_versions_sized(
    target: usize,
    version_ids: *const c_char,
    opts: *const DefraP2PSyncOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    let limit = match timeout(opts.timeout_ms) {
        Ok(limit) => limit,
        Err(error) => return error,
    };
    p2p::defra_p2p_collection_sync_versions(target, version_ids, c_ptr(&limit), opts.identity)
}

/// Syncs the documents in `doc_ids`, comma-separated, of `collection` from
/// connected peers.
///
/// # Safety
///
/// As [`defra_p2p_collection_sync_branchable_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_document_sync_sized(
    target: usize,
    collection: *const c_char,
    doc_ids: *const c_char,
    opts: *const DefraP2PSyncOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    let limit = match timeout(opts.timeout_ms) {
        Ok(limit) => limit,
        Err(error) => return error,
    };
    p2p::defra_p2p_document_sync(target, collection, doc_ids, c_ptr(&limit), opts.identity)
}

/// Adds a replicator at `addresses` for `collections`, both comma-separated.
///
/// # Safety
///
/// Every string must be null or a valid null-terminated string; `opts` must
/// be null or point to `opts_size` readable bytes whose strings are valid.
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_replicator_add_sized(
    target: usize,
    collections: *const c_char,
    addresses: *const c_char,
    opts: *const DefraReplicatorAddOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    if opts.filters_json.is_null() {
        p2p::defra_p2p_replicator_add(target, collections, addresses, opts.identity)
    } else {
        p2p::defra_p2p_replicator_add_with_filter(
            target,
            collections,
            addresses,
            opts.filters_json,
            opts.identity,
        )
    }
}

/// Deletes replicator `id`, or stops it replicating the collections named.
///
/// # Safety
///
/// As [`defra_p2p_replicator_add_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_p2p_replicator_delete_sized(
    target: usize,
    id: *const c_char,
    opts: *const DefraReplicatorDeleteOptions,
    opts_size: u32,
) -> DefraResult {
    let opts = options!(opts, opts_size);
    p2p::defra_p2p_replicator_delete(target, opts.collections, id, opts.identity)
}
