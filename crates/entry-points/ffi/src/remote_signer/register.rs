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
use crate::state::{IdentityState, IDENTITIES};
use crate::types::{c_str_to_string, DefraNewIdentityResult};

use super::{FfiRemoteSigner, HostCallback};

/// The registration of an identity whose public key is hex.
///
/// # Safety
/// The strings are null or valid null-terminated UTF-8 strings.
pub(crate) unsafe fn register_hex(
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
pub(crate) unsafe fn register_bytes(
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
