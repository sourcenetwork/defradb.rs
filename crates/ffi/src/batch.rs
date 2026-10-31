//! FFI batch signing functions.
//!
//! These expose the batch CID collection and signing to Go callers.

use std::ffi::c_char;

use defra_core::batch_signing::{BatchOptions, BatchScope, MerkleScheme};

use crate::ffi_entry;
use crate::helpers::get_rt;
use crate::state::NODES;
use crate::try_ffi;
use crate::types::{c_str_to_string, FfiResult};

/// Start (or reset) batch CID collection.
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
/// * `identity_did` - Optional DID string (null to use node identity)
/// * `session_id` - Optional unique session ID for concurrent batches.
///   If null, falls back to the node's public key hex (single-session mode).
///
/// # Safety
///
/// String pointers must be either null or valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn batch_start(
    node_ptr: usize,
    identity_did: *const c_char,
    session_id: *const c_char,
) -> FfiResult {
    batch_start_with_options(node_ptr, identity_did, session_id, std::ptr::null())
}

/// [`batch_start`], collecting the blocks `batch_scope` names: `"Composite"`
/// (null; each document's composite block, as Go Shinzo collects them) or
/// `"Full"` (every block a write produces).
///
/// # Safety
///
/// String pointers must be either null or valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn batch_start_with_options(
    node_ptr: usize,
    identity_did: *const c_char,
    session_id: *const c_char,
    batch_scope: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let _rt = try_ffi!(get_rt());
        let scope: BatchScope = try_ffi!(option(batch_scope, "batch_scope"));
        let Some((_, key)) = signing_session(node_ptr, identity_did, session_id) else {
            return FfiResult::error("no signing identity available");
        };
        defra_core::batch_signing::batch_start_with(&key, scope);
        FfiResult::ok()
    }
}

/// Sign all collected batch CIDs and return the batch signature as JSON.
///
/// Returns JSON with fields: sig_type, identity, value (hex), merkle_root (hex), cid_count.
///
/// # Arguments
///
/// * `node_ptr` - Handle to the node
/// * `identity_did` - Optional DID string (null to use node identity)
/// * `session_id` - Optional unique session ID (must match the one passed to `batch_start`).
///   If null, falls back to the node's public key hex.
///
/// # Safety
///
/// String pointers must be either null or valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn batch_sign(
    node_ptr: usize,
    identity_did: *const c_char,
    session_id: *const c_char,
) -> FfiResult {
    batch_sign_with_options(node_ptr, identity_did, session_id, std::ptr::null())
}

/// [`batch_sign`] over the Merkle tree `merkle_scheme` names: `"GoShinzo"`
/// (null; the tree Go Shinzo nodes verify), `"DomainSeparated"` or
/// `"RustLegacy"`.
///
/// # Safety
///
/// String pointers must be either null or valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn batch_sign_with_options(
    node_ptr: usize,
    identity_did: *const c_char,
    session_id: *const c_char,
    merkle_scheme: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let _rt = try_ffi!(get_rt());
        let scheme: MerkleScheme = try_ffi!(option(merkle_scheme, "merkle_scheme"));
        let Some((config, key)) = signing_session(node_ptr, identity_did, session_id) else {
            return FfiResult::error("no signing identity available");
        };
        let Some((scope, cids)) = defra_core::batch_signing::batch_take(&key) else {
            return FfiResult::error("no batch session active");
        };
        if cids.is_empty() {
            return FfiResult::error("batch has no CIDs");
        }

        match crypto::batch::sign_batch_with(&cids, &config, BatchOptions { scheme, scope }) {
            Ok(sig) => match serde_json::to_string(&sig) {
                Ok(json) => FfiResult::success(json),
                Err(e) => FfiResult::error(format!("failed to serialize batch signature: {}", e)),
            },
            Err(e) => FfiResult::error(format!("batch sign failed: {}", e)),
        }
    }
}

/// The signer for `identity_did` (or the node's identity) and the session
/// key a batch is collected under.
///
/// # Safety
///
/// Both pointers must be null or valid null-terminated UTF-8 strings.
unsafe fn signing_session(
    node_ptr: usize,
    identity_did: *const c_char,
    session_id: *const c_char,
) -> Option<(defra_core::signing::SigningConfig, String)> {
    let identity_str = c_str_to_string(identity_did);
    let session_str = c_str_to_string(session_id);
    let (node_did, signing_enabled) = NODES
        .get(node_ptr, |state| {
            (state.identity_did(), state.signing_enabled)
        })
        .unwrap_or((None, false));
    let config = defra_core::signing::resolve_signing_config_with_flag(
        identity_str.as_deref(),
        node_did.as_deref(),
        signing_enabled,
    )?;
    let key = session_str.unwrap_or_else(|| config.public_key_hex.clone());
    Some((config, key))
}

/// The option variant `name` names, or its default when null.
///
/// # Safety
///
/// `name` must be null or a valid null-terminated UTF-8 string.
unsafe fn option<T>(name: *const c_char, argument: &str) -> Result<T, FfiResult>
where
    T: Default + serde::de::DeserializeOwned,
{
    match c_str_to_string(name).filter(|name| !name.is_empty()) {
        Some(name) => serde_json::from_value(serde_json::Value::String(name.clone()))
            .map_err(|_| FfiResult::error(format!("invalid {argument}: {name}"))),
        None => Ok(T::default()),
    }
}

/// Start (or reset) collecting the CIDs of the blocks that writes tagged
/// with `session_id` (their `batch_session_id`) produce, of the kinds
/// `batch_scope` names (null: composites, as Go Shinzo collects them), for a
/// caller that signs the batch itself. No signing identity is needed.
///
/// # Safety
///
/// `session_id` must be a valid null-terminated UTF-8 string; `batch_scope`
/// null or such a string.
#[no_mangle]
pub unsafe extern "C" fn batch_collect_start(
    session_id: *const c_char,
    batch_scope: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let session = try_ffi!(crate::helpers::require_c_str(session_id, "session_id"));
        let scope: BatchScope = try_ffi!(option(batch_scope, "batch_scope"));
        defra_core::batch_signing::batch_start_with(&session, scope);
        FfiResult::ok()
    }
}

/// End the collection `session_id` names and return its CIDs as a JSON
/// array of CID strings, in the order they were collected.
///
/// # Safety
///
/// `session_id` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn batch_collect_take(session_id: *const c_char) -> FfiResult {
    ffi_entry! {
        let session = try_ffi!(crate::helpers::require_c_str(session_id, "session_id"));
        let Some((_, cids)) = defra_core::batch_signing::batch_take(&session) else {
            return FfiResult::error("no batch session active");
        };
        let cids: Vec<String> = cids.iter().map(ToString::to_string).collect();
        match serde_json::to_string(&cids) {
            Ok(json) => FfiResult::success(json),
            Err(e) => FfiResult::error(format!("failed to serialize batch CIDs: {e}")),
        }
    }
}
