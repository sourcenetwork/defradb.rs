//! Registering an identity whose private key the host keeps: with a
//! synchronous or an asynchronous signing callback, and its public key as hex
//! or as raw bytes. Every form shares one body per key encoding, and answers
//! with the identity's handle, which calls take as `identity_ptr` like one
//! from `defra_identity_new`.

use std::ffi::{c_char, CString};
use std::sync::Arc;

use crate::acp::identity_ops::{
    decode_and_validate_public_key, parse_signing_key_type, validate_public_key_bytes,
};
use crate::ffi_entry;
use crate::state::{IdentityState, IDENTITIES};
use crate::types::{c_str_to_string, DefraNewIdentityResult};

use super::{DefraRemoteSignAsyncCallback, DefraRemoteSignCallback, FfiRemoteSigner, HostCallback};

/// Register an identity whose private key stays outside DefraDB.
///
/// The host provides the DID, public key, key type, and a signing callback.
/// This supports device-bound keys (for example Secure Enclave P-256 keys) and
/// remote identity providers such as Orbis without exporting private key bytes.
///
/// Swift can import this symbol directly from the generated Apple package.
/// The callback is required and uses a plain C function-pointer ABI.
///
/// While a DID has live identity handles, its signer is fixed: registering
/// the DID again with another `signer_handle` or callback fails until every
/// handle for it is freed; with the same `signer_handle` and callback it
/// gives another handle for the same signer. `signer_handle` must stay
/// valid, and `callback` callable, until every identity handle naming them
/// is freed and every node started with one (or given one as its identity)
/// is closed or given another identity: a node keeps signing through them
/// after the handle is freed.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn defra_identity_new_remote(
    did: *const c_char,
    public_key_hex: *const c_char,
    key_type: *const c_char,
    signer_handle: usize,
    callback: DefraRemoteSignCallback,
) -> DefraNewIdentityResult {
    ffi_entry! {
        // SAFETY: the strings are null or valid C strings from the caller.
        unsafe { register_hex(did, public_key_hex, key_type, signer_handle, HostCallback::Sync(callback)) }
    }
}

/// Register an identity whose public key is already available as raw bytes.
///
/// This is intended for Swift and Objective-C callers that already have the
/// device public key as `Data` or `UnsafePointer<UInt8>`. The callback contract
/// matches [`defra_identity_new_remote`].
///
/// Public key format:
/// - `secp256r1` / `secp256k1`: uncompressed SEC1 / X9.63 bytes
/// - `ed25519`: raw 32-byte public key
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn defra_identity_new_remote_bytes(
    did: *const c_char,
    public_key_ptr: *const u8,
    public_key_len: usize,
    key_type: *const c_char,
    signer_handle: usize,
    callback: DefraRemoteSignCallback,
) -> DefraNewIdentityResult {
    ffi_entry! {
        // SAFETY: the strings are null or valid C strings, and the key is
        // null or `public_key_len` readable bytes, from the caller.
        unsafe {
            register_bytes(
                did,
                public_key_ptr,
                public_key_len,
                key_type,
                signer_handle,
                HostCallback::Sync(callback),
            )
        }
    }
}

/// [`defra_identity_new_remote`] for a host whose signer answers later: the
/// callback starts each signature and returns at once, and the host reports
/// the result with `defra_remote_sign_complete`. No DefraDB thread waits
/// while the host signs, so this is the form for a host that signs on its
/// own queue or over the network, and for a node running a single worker.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn defra_identity_new_remote_async(
    did: *const c_char,
    public_key_hex: *const c_char,
    key_type: *const c_char,
    signer_handle: usize,
    callback: DefraRemoteSignAsyncCallback,
) -> DefraNewIdentityResult {
    ffi_entry! {
        // SAFETY: the strings are null or valid C strings from the caller.
        unsafe { register_hex(did, public_key_hex, key_type, signer_handle, HostCallback::Async(callback)) }
    }
}

/// [`defra_identity_new_remote_bytes`] with an asynchronous callback, as
/// [`defra_identity_new_remote_async`].
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn defra_identity_new_remote_bytes_async(
    did: *const c_char,
    public_key_ptr: *const u8,
    public_key_len: usize,
    key_type: *const c_char,
    signer_handle: usize,
    callback: DefraRemoteSignAsyncCallback,
) -> DefraNewIdentityResult {
    ffi_entry! {
        // SAFETY: as for `defra_identity_new_remote_bytes`.
        unsafe {
            register_bytes(
                did,
                public_key_ptr,
                public_key_len,
                key_type,
                signer_handle,
                HostCallback::Async(callback),
            )
        }
    }
}

/// The registration of an identity whose public key is hex.
///
/// # Safety
/// The strings are null or valid null-terminated UTF-8 strings.
unsafe fn register_hex(
    did: *const c_char,
    public_key_hex: *const c_char,
    key_type: *const c_char,
    signer_handle: usize,
    callback: HostCallback,
) -> DefraNewIdentityResult {
    let result = (|| {
        let did_str = c_str_to_string(did).ok_or("invalid did parameter")?;
        let pub_hex = c_str_to_string(public_key_hex).ok_or("invalid public_key_hex parameter")?;
        let key_type_str = c_str_to_string(key_type).unwrap_or_else(|| "secp256r1".to_string());
        let signing_key_type = parse_signing_key_type(&key_type_str)?;

        let public_key_bytes =
            decode_and_validate_public_key(&did_str, &pub_hex, signing_key_type)?;

        store(
            did_str,
            public_key_bytes,
            pub_hex,
            signing_key_type,
            signer_handle,
            callback,
        )
    })();
    match result {
        Ok(handle) => DefraNewIdentityResult::success(handle),
        Err(error) => DefraNewIdentityResult::error(error),
    }
}

/// The registration of an identity whose public key is raw bytes.
///
/// # Safety
/// The strings are null or valid null-terminated UTF-8 strings, and
/// `public_key_ptr` is null or valid for `public_key_len` bytes.
unsafe fn register_bytes(
    did: *const c_char,
    public_key_ptr: *const u8,
    public_key_len: usize,
    key_type: *const c_char,
    signer_handle: usize,
    callback: HostCallback,
) -> DefraNewIdentityResult {
    let result = (|| {
        let did_str = c_str_to_string(did).ok_or("invalid did parameter")?;
        if public_key_ptr.is_null() || public_key_len == 0 {
            return Err("invalid public_key_bytes parameter".to_string());
        }
        if public_key_len > 256 {
            return Err(format!(
                "public_key_len {} exceeds maximum 256",
                public_key_len
            ));
        }
        let key_type_str = c_str_to_string(key_type).unwrap_or_else(|| "secp256r1".to_string());
        let signing_key_type = parse_signing_key_type(&key_type_str)?;
        // SAFETY: `public_key_ptr` is non-null and `public_key_len` is
        // bounded (checked above). The caller guarantees the pointer is
        // valid for `public_key_len` bytes.
        let public_key_bytes = std::slice::from_raw_parts(public_key_ptr, public_key_len).to_vec();

        validate_public_key_bytes(&did_str, &public_key_bytes, signing_key_type)?;

        store(
            did_str,
            public_key_bytes.clone(),
            hex::encode(&public_key_bytes),
            signing_key_type,
            signer_handle,
            callback,
        )
    })();
    match result {
        Ok(handle) => DefraNewIdentityResult::success(handle),
        Err(error) => DefraNewIdentityResult::error(error),
    }
}

/// Stores `did`'s signing configuration, which signs through the host's
/// `callback`.
fn store(
    did: String,
    public_key_bytes: Vec<u8>,
    public_key_hex: String,
    key_type: defra_core::signing::SigningKeyType,
    signer_handle: usize,
    callback: HostCallback,
) -> Result<usize, String> {
    let signer = FfiRemoteSigner::new(
        CString::new(did.clone()).map_err(|_| "did contains an embedded null byte".to_string())?,
        CString::new(key_type.to_string())
            .map_err(|_| "key_type contains an embedded null byte".to_string())?,
        signer_handle,
        callback,
    );

    let hold = defra_core::signing::IdentityHold::new(
        &did,
        defra_core::signing::SigningConfig {
            key_type,
            private_key_bytes: Vec::new(),
            public_key_bytes,
            public_key_hex: public_key_hex.clone(),
            remote_signer: Some(Arc::new(signer)),
            signing_authorization: None,
        },
    )
    .map_err(|error| error.to_string())?;

    Ok(IDENTITIES.insert(IdentityState {
        did,
        key_type: Some(key_type),
        private_key: None,
        public_key_hex,
        hold: Some(hold),
        bearer_token: None,
    }))
}
