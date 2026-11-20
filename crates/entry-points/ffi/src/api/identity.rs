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
    /// Reserved, so the struct has room to grow: leave it zero. It never
    /// becomes a field; options go after it.
    pub reserved: u64,
}

/// Options of `defra_identity_new_remote`: the public key as hex or as
/// bytes, and exactly one of the two signing callbacks.
///
/// Public key format, as bytes or hex-encoded:
/// - `secp256r1` / `secp256k1`: uncompressed SEC1 / X9.63 bytes
///   (`0x04 || X || Y`)
/// - `ed25519`: the raw 32-byte public key
///
/// Swift and Objective-C callers that already have the device public key as
/// `Data` or `UnsafePointer<UInt8>` set `public_key` and `public_key_len`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DefraRemoteIdentityOptions {
    /// The key's type (null or empty for "secp256r1").
    pub key_type: *const c_char,
    /// The public key, hex-encoded.
    pub public_key_hex: *const c_char,
    /// The public key's bytes, used when `public_key_hex` is null (at most
    /// 256 bytes).
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

/// Registers identity `did`, whose private key stays outside DefraDB: the
/// host keeps it and signs with it through the callback the options give.
/// This supports device-bound keys (for example Secure Enclave P-256 keys)
/// and remote identity providers such as Orbis, without exporting private
/// key bytes.
///
/// `sign` answers before it returns. `sign_async` starts each signature and
/// returns at once, and the host reports the result with
/// `defra_remote_sign_complete`; no DefraDB thread waits while the host
/// signs, so it is the form for a host that signs on its own queue or over
/// the network, and for a node running a single worker.
///
/// While a DID has live identity handles, its signer is fixed: registering
/// the DID again with another `signer_handle` or callback fails until every
/// handle for it is freed; with the same `signer_handle` and callback it
/// gives another handle for the same signer. `signer_handle` must stay
/// valid, and the callback callable, until every identity handle naming them
/// is freed and every node started with one (or given one as its identity)
/// is closed or given another identity: a node keeps signing through them
/// after the handle is freed.
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
