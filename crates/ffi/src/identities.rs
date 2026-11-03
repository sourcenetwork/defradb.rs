//! Identities: created, imported, exported and released by handle.
//!
//! A call that acts as someone takes the handle of their identity. Each
//! identity is also registered in the signing store under its DID, which is
//! how the requests it makes sign their blocks.

use std::ffi::c_char;

use identity::RawIdentity;

use crate::state::{IdentityState, IDENTITIES, NODES};
use crate::target::{go_string, resolve_target};
use crate::types::{DefraNewIdentityResult, DefraResult};
use crate::{ffi_entry, try_ffi};

/// Length of a secp256k1 private key in bytes.
const SECP256K1_PRIVATE_KEY_LEN: usize = 32;

/// What exporting the null identity handle reports.
const ERR_IDENTITY_IS_NIL: &str = "identity is nil";

/// What exporting an identity whose private key the host keeps reports.
const ERR_NO_PRIVATE_KEY: &str = "identity does not contain a private key";

/// What importing a private key of the wrong length reports.
const ERR_INVALID_PRIVATE_KEY_LENGTH: &str = "private key must be 32 bytes";

/// Creates an identity with a new key of type `key_type`: "secp256k1" (the
/// default, for null or empty), "secp256r1" or "ed25519".
///
/// # Safety
///
/// `key_type` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_identity_new(key_type: *const c_char) -> DefraNewIdentityResult {
    ffi_entry! {
        let key_type = match go_string(key_type).as_str() {
            "" | "secp256k1" => crypto::KeyType::Secp256k1,
            "secp256r1" => crypto::KeyType::Secp256r1,
            "ed25519" => crypto::KeyType::Ed25519,
            other => return DefraNewIdentityResult::error(format!("unsupported key type: {}", other)),
        };
        let identity = crypto::generate_key(key_type)
            .map_err(|error| format!("failed to generate key: {}", error))
            .and_then(|key| {
                RawIdentity::from_bytes(key_type, &key.raw_owned())
                    .map_err(|error| format!("failed to create identity: {}", error))
            });
        insert(identity)
    }
}

/// Imports an identity from a hex-encoded 32-byte secp256k1 private key.
///
/// # Safety
///
/// `private_key_hex` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_identity_new_from_private_key(
    private_key_hex: *const c_char,
) -> DefraNewIdentityResult {
    ffi_entry! {
        let identity = hex::decode(go_string(private_key_hex))
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                if bytes.len() != SECP256K1_PRIVATE_KEY_LEN {
                    return Err(ERR_INVALID_PRIVATE_KEY_LENGTH.to_string());
                }
                RawIdentity::from_bytes(crypto::KeyType::Secp256k1, &bytes)
                    .map_err(|error| error.to_string())
            });
        insert(identity)
    }
}

/// Registers `identity` and answers with its handle.
fn insert(identity: Result<RawIdentity, String>) -> DefraNewIdentityResult {
    match identity.and_then(|identity| IdentityState::register(&identity)) {
        Ok(state) => DefraNewIdentityResult::success(IDENTITIES.insert(state)),
        Err(error) => DefraNewIdentityResult::error(error),
    }
}

/// Returns the identity's private key, hex-encoded.
#[no_mangle]
pub extern "C" fn defra_identity_export_private_key(identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        if identity_ptr == 0 {
            return DefraResult::error(ERR_IDENTITY_IS_NIL);
        }
        match IDENTITIES.get(identity_ptr) {
            Some(identity) => match &identity.private_key {
                Some(key) => DefraResult::success(hex::encode(key.as_slice())),
                None => DefraResult::error(ERR_NO_PRIVATE_KEY),
            },
            None => DefraResult::error(crate::target::ERR_INVALID_HANDLE),
        }
    }
}

/// Returns the identity's DID (`did:key:...`), the name ACP calls take for
/// an actor. Not in Go's C API, which offers no way to read it.
#[no_mangle]
pub extern "C" fn defra_identity_did(identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        if identity_ptr == 0 {
            return DefraResult::error(ERR_IDENTITY_IS_NIL);
        }
        match IDENTITIES.get(identity_ptr) {
            Some(identity) => DefraResult::success(identity.did.clone()),
            None => DefraResult::error(crate::target::ERR_INVALID_HANDLE),
        }
    }
}

/// Releases an identity handle and its hold on the identity's private key,
/// which leaves memory once no other handle or node holds it. Releasing the
/// null handle, or one already released, does nothing.
#[no_mangle]
pub extern "C" fn defra_identity_free(identity_ptr: usize) {
    ffi_entry! {
        IDENTITIES.release(identity_ptr);
    }
}

/// Returns the node's public identity as JSON, `{"PublicKey": ..., "DID":
/// ...}`, for a node or a transaction on it: the one it started with, or the
/// one `defra_node_set_identity` gave it since. A node without an identity
/// answers with a message saying so.
#[no_mangle]
pub extern "C" fn defra_node_identity(node_ptr: usize) -> DefraResult {
    ffi_entry! {
        let node = try_ffi!(resolve_target(node_ptr)).node();
        let Some(did) = NODES.get(node, |state| state.identity_did()).flatten() else {
            return DefraResult::success("Node has no identity assigned to it.");
        };
        // An identity a bearer token stands for has no public key here.
        let public_key = defra_core::signing::get_identity(&did)
            .map(|config| config.public_key_hex.clone())
            .unwrap_or_default();
        DefraResult::success(
            serde_json::json!({
                "PublicKey": public_key,
                "DID": did,
            })
            .to_string(),
        )
    }
}
