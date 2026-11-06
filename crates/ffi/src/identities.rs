//! Identities: created, imported, exported and released by handle.
//!
//! A call that acts as someone takes the handle of their identity. Each
//! identity is also registered in the signing store under its DID, which is
//! how the requests it makes sign their blocks.

use std::ffi::c_char;

use identity::RawIdentity;
use zeroize::Zeroizing;

use crate::state::{IdentityState, IDENTITIES, NODES};
use crate::target::{go_string, resolve_target};
use crate::types::{DefraNewIdentityResult, DefraResult};
use crate::{ffi_entry, try_ffi};

/// What exporting the null identity handle reports.
const ERR_IDENTITY_IS_NIL: &str = "identity is nil";

/// What exporting an identity whose private key the host keeps reports.
const ERR_NO_PRIVATE_KEY: &str = "identity does not contain a private key";

/// Length in bytes of a `key_type` private key as it is exported: a 32-byte
/// secp256k1 or secp256r1 scalar, or an ed25519 seed followed by its public
/// key, 64 bytes, as Go keeps it.
fn private_key_len(key_type: crypto::KeyType) -> usize {
    match key_type {
        crypto::KeyType::Ed25519 => 64,
        _ => 32,
    }
}

/// Creates an identity with a new key of type `key_type`: "secp256k1" (the
/// default, for null or empty), "secp256r1" or "ed25519".
///
/// # Safety
///
/// `key_type` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_identity_new(key_type: *const c_char) -> DefraNewIdentityResult {
    ffi_entry! {
        let key_type = match parse_key_type(key_type) {
            Ok(key_type) => key_type,
            Err(error) => return DefraNewIdentityResult::error(error),
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

/// The key type a C caller names: "secp256k1" (the default, for null or
/// empty), "secp256r1" or "ed25519".
///
/// # Safety
///
/// `key_type` must be null or a valid null-terminated string.
unsafe fn parse_key_type(key_type: *const c_char) -> Result<crypto::KeyType, String> {
    match go_string(key_type).as_str() {
        "" | "secp256k1" => Ok(crypto::KeyType::Secp256k1),
        "secp256r1" => Ok(crypto::KeyType::Secp256r1),
        "ed25519" => Ok(crypto::KeyType::Ed25519),
        other => Err(format!("unsupported key type: {}", other)),
    }
}

/// Imports an identity from a hex-encoded 32-byte secp256k1 private key as
/// Go's `NewIdentityFromPrivateKey` does. Other key types import with
/// [`defra_identity_new_from_typed_private_key`].
///
/// # Safety
///
/// `private_key_hex` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_identity_new_from_private_key(
    private_key_hex: *const c_char,
) -> DefraNewIdentityResult {
    ffi_entry! {
        insert(import(crypto::KeyType::Secp256k1, private_key_hex))
    }
}

/// Imports an identity of `key_type` (as [`defra_identity_new`] names it)
/// from the hex-encoded private key [`defra_identity_export_private_key`]
/// gave. Not in Go's C API, which imports secp256k1 keys only.
///
/// # Safety
///
/// `key_type` and `private_key_hex` must be null or valid null-terminated
/// strings.
#[no_mangle]
pub unsafe extern "C" fn defra_identity_new_from_typed_private_key(
    key_type: *const c_char,
    private_key_hex: *const c_char,
) -> DefraNewIdentityResult {
    ffi_entry! {
        insert(parse_key_type(key_type).and_then(|key_type| import(key_type, private_key_hex)))
    }
}

/// The identity of `key_type` whose private key `private_key_hex` encodes.
///
/// # Safety
///
/// `private_key_hex` must be null or a valid null-terminated string.
unsafe fn import(
    key_type: crypto::KeyType,
    private_key_hex: *const c_char,
) -> Result<RawIdentity, String> {
    let bytes =
        Zeroizing::new(hex::decode(go_string(private_key_hex)).map_err(|error| error.to_string())?);
    let expected = private_key_len(key_type);
    if bytes.len() != expected {
        return Err(format!(
            "a {} private key must be {} bytes, got {}",
            key_type,
            expected,
            bytes.len()
        ));
    }
    RawIdentity::from_bytes(key_type, &bytes).map_err(|error| error.to_string())
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
