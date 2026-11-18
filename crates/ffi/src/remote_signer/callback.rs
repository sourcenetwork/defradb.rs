//! The C callbacks a host signs through, and what their answers mean.

use std::ffi::{c_char, c_int};

use defra_core::signing::SignFailure;

/// The longest signature a host may report, in bytes.
pub(crate) const MAX_REMOTE_SIGNATURE_LEN: usize = 512;

/// The status an asynchronous signature completes with when the request did
/// not reach the signer or its answer did not come back. Signing the same
/// bytes again is safe, so DefraDB may ask again.
pub const DEFRA_SIGN_RETRYABLE: c_int = 2;

/// Host callback used for non-exportable signing keys.
///
/// The host app keeps the private key and signs the payload on demand. This is
/// intended for device-bound keys such as Secure Enclave-backed P-256 keys.
///
/// Contract:
/// - `signer_handle` is an opaque host-managed context value. It must stay
///   valid, and the callback callable, until every node using the DID is
///   closed or given another identity, not just until the identity's handle
///   is freed: a node keeps its hold on the identity, and signs through the
///   callback, after `defra_identity_free`. Freeing the context together with
///   the handle leaves such a node calling into freed memory.
/// - `did` is the registered signing DID for this key.
/// - `key_type` is the registered Defra key type, for example `secp256r1`.
/// - `payload_ptr` / `payload_len` describe the exact message bytes to sign.
/// - The callback must write the signature bytes into `out_signature_ptr`,
///   set `out_signature_len`, and return `0` on success.
///
/// The callback blocks the thread that calls it, so a wave of signatures (the
/// blocks of a write) calls it on a thread set aside for blocking work. A
/// host whose signer answers later registers a
/// [`DefraRemoteSignAsyncCallback`] instead, which holds no thread.
///
/// Signature format:
/// - For ECDSA keys (`secp256r1`, `secp256k1`), Defra expects an ASN.1 DER /
///   X9.62 ECDSA signature, not raw `r || s`.
/// - For Secure Enclave P-256 keys on Apple platforms, use
///   `SecKeyCreateSignature(..., .ecdsaSignatureMessageX962SHA256, ...)`.
pub type DefraRemoteSignCallback = unsafe extern "C" fn(
    signer_handle: usize,
    did: *const c_char,
    key_type: *const c_char,
    payload_ptr: *const u8,
    payload_len: usize,
    out_signature_ptr: *mut u8,
    out_signature_capacity: usize,
    out_signature_len: *mut usize,
) -> c_int;

/// Host callback that starts a signature and returns without waiting for it.
///
/// Contract:
/// - `signer_handle`, `did` and `key_type` are as for
///   [`DefraRemoteSignCallback`], and so is how long `signer_handle` and the
///   callback must stay valid: until every node using the DID is closed or
///   given another identity, not just until the handle is freed.
/// - `payload_ptr` / `payload_len` are the exact message bytes to sign. They,
///   `did` and `key_type` are valid only until the callback returns: copy the
///   payload if the signature is made later.
/// - `completion` names this one request. Return `0` once the signature is
///   under way, then report its result exactly once, from any thread, with
///   `defra_remote_sign_complete(completion, ...)`; reporting it from inside
///   the callback is fine too. Return any other value when the signature
///   cannot be started; the request then fails at once and must not be
///   reported.
/// - The callback runs on a DefraDB thread that serves other work: it must
///   return promptly and must not call another DefraDB function that waits.
///
/// Signature format: as for [`DefraRemoteSignCallback`].
pub type DefraRemoteSignAsyncCallback = unsafe extern "C" fn(
    signer_handle: usize,
    did: *const c_char,
    key_type: *const c_char,
    payload_ptr: *const u8,
    payload_len: usize,
    completion: usize,
) -> c_int;

/// The failure a host's non-zero `status` reports. Only an asynchronous
/// answer can be [`DEFRA_SIGN_RETRYABLE`]: the synchronous callback's
/// contract never gave its statuses a meaning.
pub(super) fn failed(status: c_int, asynchronous: bool) -> SignFailure {
    let message = format!("remote signer callback failed with status {}", status);
    if asynchronous && status == DEFRA_SIGN_RETRYABLE {
        SignFailure::transient(message)
    } else {
        SignFailure::refused(message)
    }
}

/// The `len` signature bytes a host reported, which `read` copies out once
/// they passed the checks every answer gets.
pub(super) fn signature<R>(len: usize, read: R) -> Result<Vec<u8>, SignFailure>
where
    R: FnOnce() -> Vec<u8>,
{
    if len == 0 {
        return Err(SignFailure::refused(
            "remote signer callback returned an empty signature",
        ));
    }
    if len > MAX_REMOTE_SIGNATURE_LEN {
        return Err(SignFailure::refused(format!(
            "remote signer callback reported {} bytes, exceeding the {} byte limit",
            len, MAX_REMOTE_SIGNATURE_LEN
        )));
    }
    Ok(read())
}
