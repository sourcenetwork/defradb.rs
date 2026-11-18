//! Identities: new keys, imported keys, bearer tokens, and keys the host
//! keeps behind a signing callback.

use std::ffi::{c_char, c_int};

use super::{options, zero_default};
use crate::remote_signer::{register_bytes, register_hex, HostCallback};
use crate::types::DefraNewIdentityResult;
use crate::{acp, ffi_entry, identities};

/// Options of `defra_identity_new` and `defra_identity_new_from_private_key`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraIdentityNewOptions {
    /// "secp256k1" (the default, for null or empty), "secp256r1" or "ed25519".
    pub key_type: *const c_char,
}

/// Options of `defra_identity_new_from_bearer_token`. Nothing is optional
/// yet; the struct is where options will go.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraIdentityBearerOptions {
    /// Reserved: leave zero.
    pub reserved: i32,
}

/// Options of `defra_identity_new_remote`: the public key as hex or as
/// bytes, and exactly one of the two signing callbacks.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraRemoteIdentityOptions {
    /// The key's type (null or empty for "secp256r1").
    pub key_type: *const c_char,
    /// The public key, hex-encoded.
    pub public_key_hex: *const c_char,
    /// The public key's bytes, used when `public_key_hex` is null.
    pub public_key: *const u8,
    /// Length of `public_key` in bytes.
    pub public_key_len: usize,
    /// Passed back to the callback on every signing request.
    pub signer_handle: usize,
    /// A signer that answers before it returns: a `DefraRemoteSignCallback`,
    /// spelled out so C sees a nullable function pointer.
    pub sign: Option<
        unsafe extern "C" fn(
            signer_handle: usize,
            did: *const c_char,
            key_type: *const c_char,
            payload_ptr: *const u8,
            payload_len: usize,
            out_signature_ptr: *mut u8,
            out_signature_capacity: usize,
            out_signature_len: *mut usize,
        ) -> c_int,
    >,
    /// A signer that answers later, through `defra_remote_sign_complete`: a
    /// `DefraRemoteSignAsyncCallback`, spelled out as `sign` is.
    pub sign_async: Option<
        unsafe extern "C" fn(
            signer_handle: usize,
            did: *const c_char,
            key_type: *const c_char,
            payload_ptr: *const u8,
            payload_len: usize,
            completion: usize,
        ) -> c_int,
    >,
}

zero_default!(
    DefraIdentityNewOptions,
    DefraIdentityBearerOptions,
    DefraRemoteIdentityOptions,
);

/// Creates an identity with a new key.
///
/// # Safety
///
/// `opts` must be null or point to `opts_size` readable bytes whose strings
/// are valid.
#[no_mangle]
pub unsafe extern "C" fn defra_identity_new_sized(
    opts: *const DefraIdentityNewOptions,
    opts_size: u32,
) -> DefraNewIdentityResult {
    let opts = options!(opts, opts_size, DefraNewIdentityResult::error);
    identities::defra_identity_new(opts.key_type)
}

/// Imports an identity from the hex-encoded private key
/// `defra_identity_export_private_key` gave.
///
/// # Safety
///
/// `private_key_hex` must be null or a valid null-terminated string; `opts`
/// as [`defra_identity_new_sized`].
#[no_mangle]
pub unsafe extern "C" fn defra_identity_new_from_private_key_sized(
    private_key_hex: *const c_char,
    opts: *const DefraIdentityNewOptions,
    opts_size: u32,
) -> DefraNewIdentityResult {
    let opts = options!(opts, opts_size, DefraNewIdentityResult::error);
    identities::defra_identity_new_from_typed_private_key(opts.key_type, private_key_hex)
}

/// An identity for `did` that signs with `bearer_token`.
///
/// # Safety
///
/// `did` and `bearer_token` must be null or valid null-terminated strings;
/// `opts` must be null or point to `opts_size` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_identity_new_from_bearer_token_sized(
    did: *const c_char,
    bearer_token: *const c_char,
    opts: *const DefraIdentityBearerOptions,
    opts_size: u32,
) -> DefraNewIdentityResult {
    let _ = options!(opts, opts_size, DefraNewIdentityResult::error);
    acp::defra_identity_new_from_bearer_token(did, bearer_token)
}

/// Registers identity `did`, whose private key the host keeps and signs
/// with through the callback the options give.
///
/// # Safety
///
/// `did` must be null or a valid null-terminated string; `opts` must be null
/// or point to `opts_size` readable bytes, whose strings are valid and whose
/// `public_key` is null or `public_key_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn defra_identity_new_remote_sized(
    did: *const c_char,
    opts: *const DefraRemoteIdentityOptions,
    opts_size: u32,
) -> DefraNewIdentityResult {
    let opts = options!(opts, opts_size, DefraNewIdentityResult::error);
    let callback = match (opts.sign, opts.sign_async) {
        (Some(sign), None) => HostCallback::Sync(sign),
        (None, Some(sign)) => HostCallback::Async(sign),
        (None, None) => {
            return DefraNewIdentityResult::error("give a signing callback: sign or sign_async")
        }
        (Some(_), Some(_)) => {
            return DefraNewIdentityResult::error(
                "give one signing callback, sign or sign_async, not both",
            )
        }
    };
    ffi_entry! {
        if opts.public_key_hex.is_null() {
            register_bytes(
                did,
                opts.public_key,
                opts.public_key_len,
                opts.key_type,
                opts.signer_handle,
                callback,
            )
        } else {
            register_hex(did, opts.public_key_hex, opts.key_type, opts.signer_handle, callback)
        }
    }
}
