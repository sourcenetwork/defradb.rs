//! Registering an identity whose key the host keeps, by hex or by bytes, and
//! one a bearer token stands for.

use std::ffi::{c_char, c_int, CString};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crypto::{PrivateKey, PublicKey};
use identity::{Identity, RawIdentity};

use super::{defra_identity_new_remote, defra_identity_new_remote_bytes};
use crate::acp::{defra_identity_new_from_bearer_token, defra_node_set_identity};
use crate::identities::{defra_identity_did, defra_identity_free, defra_node_identity};
use crate::test_support::{close, cstr, node, ok, ok_json, text};
use crate::types::{defra_new_identity_result_free, DefraNewIdentityResult};

/// A secp256r1 key the host keeps. Its address is the signer handle the
/// callback receives, so each test signs with its own key.
struct HostKey {
    private: crypto::Secp256r1PrivateKey,
    did: String,
    public: Vec<u8>,
    /// How many signatures the host made through this context.
    signed: AtomicUsize,
}

impl HostKey {
    fn new() -> &'static Self {
        let private = crypto::generate_secp256r1().unwrap();
        let identity = RawIdentity::from_secp256r1(private.clone()).unwrap();
        Box::leak(Box::new(Self {
            did: identity.did().unwrap().to_string(),
            public: identity.public_key_bytes(),
            private,
            signed: AtomicUsize::new(0),
        }))
    }

    /// The same key behind a second host context, with its own handle.
    fn another_context(&self) -> &'static Self {
        Box::leak(Box::new(Self {
            private: self.private.clone(),
            did: self.did.clone(),
            public: self.public.clone(),
            signed: AtomicUsize::new(0),
        }))
    }

    fn handle(&'static self) -> usize {
        self as *const Self as usize
    }
}

unsafe extern "C" fn sign(
    signer_handle: usize,
    _did: *const c_char,
    _key_type: *const c_char,
    payload_ptr: *const u8,
    payload_len: usize,
    out_signature_ptr: *mut u8,
    out_signature_capacity: usize,
    out_signature_len: *mut usize,
) -> c_int {
    let key = &*(signer_handle as *const HostKey);
    key.signed.fetch_add(1, Ordering::SeqCst);
    let payload = std::slice::from_raw_parts(payload_ptr, payload_len);
    let Ok(signature) = key.private.sign(payload) else {
        return 2;
    };
    if signature.len() > out_signature_capacity {
        return 3;
    }
    std::ptr::copy_nonoverlapping(signature.as_ptr(), out_signature_ptr, signature.len());
    *out_signature_len = signature.len();
    0
}

/// The handle of a registration that must succeed.
fn registered(result: DefraNewIdentityResult) -> usize {
    assert_eq!(
        result.status,
        0,
        "registration failed: {}",
        text(result.error)
    );
    assert_ne!(result.identity_ptr, 0);
    result.identity_ptr
}

/// The error of a registration that must fail.
fn refused(mut result: DefraNewIdentityResult) -> String {
    assert_eq!(result.status, 1, "registration must fail");
    let error = text(result.error);
    unsafe { defra_new_identity_result_free(&mut result) };
    error
}

fn register_hex(key: &'static HostKey) -> usize {
    let did = cstr(&key.did);
    let public = cstr(&hex::encode(&key.public));
    let key_type = cstr("secp256r1");
    registered(defra_identity_new_remote(
        did.as_ptr(),
        public.as_ptr(),
        key_type.as_ptr(),
        key.handle(),
        sign,
    ))
}

#[test]
fn a_hex_registration_stores_a_signer_that_signs_with_the_hosts_key() {
    let key = HostKey::new();
    let identity = register_hex(key);
    assert_eq!(ok(defra_identity_did(identity)), key.did);

    let config = defra_core::signing::get_identity(&key.did).expect("the identity is stored");
    assert_eq!(
        config.key_type,
        defra_core::signing::SigningKeyType::Secp256r1
    );
    assert!(
        config.private_key_bytes.is_empty(),
        "the host keeps the key"
    );
    let message = b"remote-signature-test";
    let signature = config
        .remote_signer
        .as_ref()
        .expect("a remote signer")
        .sign_sync(message, None)
        .expect("the host signs");
    let public = crypto::Secp256r1PublicKey::from_bytes(&key.public).unwrap();
    assert!(public.verify(message, &signature).unwrap());

    defra_identity_free(identity);
}

/// Signs through whatever signer the store holds for `key`'s DID.
fn sign_with_stored(key: &HostKey) {
    defra_core::signing::get_identity(&key.did)
        .expect("the identity is stored")
        .remote_signer
        .as_ref()
        .expect("a remote signer")
        .sign_sync(b"which-context-signs", None)
        .expect("the host signs");
}

/// Registering one DID twice: the second registration, with another host
/// context, is refused while the first handle lives, so the first keeps
/// signing through its own context and is never re-routed to one its host
/// may free. Once the first handle is freed, the DID registers again.
#[test]
fn a_second_registration_of_a_held_did_is_refused_until_the_first_is_freed() {
    let key = HostKey::new();
    let other = key.another_context();
    let first = register_hex(key);

    let did = cstr(&key.did);
    let public = cstr(&hex::encode(&key.public));
    let key_type = cstr("secp256r1");
    let error = refused(defra_identity_new_remote(
        did.as_ptr(),
        public.as_ptr(),
        key_type.as_ptr(),
        other.handle(),
        sign,
    ));
    assert!(error.contains("different signer"), "{error}");

    sign_with_stored(key);
    assert_eq!(
        key.signed.load(Ordering::SeqCst),
        1,
        "the first context signs"
    );
    assert_eq!(
        other.signed.load(Ordering::SeqCst),
        0,
        "the refused one never does"
    );

    defra_identity_free(first);
    assert!(
        defra_core::signing::get_identity(&key.did).is_none(),
        "the refused registration held nothing"
    );

    let second = register_hex(other);
    sign_with_stored(other);
    assert_eq!(
        other.signed.load(Ordering::SeqCst),
        1,
        "a free DID takes the new context"
    );
    defra_identity_free(second);
}

#[test]
fn a_bytes_registration_stores_the_public_key() {
    let key = HostKey::new();
    let did = cstr(&key.did);
    let key_type = cstr("secp256r1");
    let identity = registered(defra_identity_new_remote_bytes(
        did.as_ptr(),
        key.public.as_ptr(),
        key.public.len(),
        key_type.as_ptr(),
        key.handle(),
        sign,
    ));

    let config = defra_core::signing::get_identity(&key.did).expect("the identity is stored");
    assert_eq!(config.public_key_bytes, key.public);
    assert_eq!(config.public_key_hex, hex::encode(&key.public));
    assert!(config.remote_signer.is_some());

    defra_identity_free(identity);
}

#[test]
fn a_registration_refuses_an_unknown_key_type_and_a_key_that_is_not_the_dids() {
    let key = HostKey::new();
    let did = cstr(&key.did);
    let public = cstr(&hex::encode(&key.public));
    let unknown = cstr("not-a-key");
    assert_eq!(
        refused(defra_identity_new_remote(
            did.as_ptr(),
            public.as_ptr(),
            unknown.as_ptr(),
            key.handle(),
            sign,
        )),
        "unsupported signing key type: not-a-key"
    );

    let other = HostKey::new();
    let other_public = cstr(&hex::encode(&other.public));
    let key_type = cstr("secp256r1");
    refused(defra_identity_new_remote(
        did.as_ptr(),
        other_public.as_ptr(),
        key_type.as_ptr(),
        key.handle(),
        sign,
    ));
    assert!(defra_core::signing::get_identity(&key.did).is_none());

    let empty: CString = cstr("");
    refused(defra_identity_new_remote_bytes(
        did.as_ptr(),
        std::ptr::null(),
        0,
        key_type.as_ptr(),
        key.handle(),
        sign,
    ));
    refused(defra_identity_new_remote(
        empty.as_ptr(),
        public.as_ptr(),
        key_type.as_ptr(),
        key.handle(),
        sign,
    ));
}

#[test]
fn a_node_takes_a_remote_identity_as_its_own() {
    let key = HostKey::new();
    let identity = register_hex(key);
    let node = node();

    ok(defra_node_set_identity(node, identity));
    let node_identity = ok_json(defra_node_identity(node));
    assert_eq!(node_identity["DID"], key.did.as_str(), "{node_identity}");

    close(node);
    defra_identity_free(identity);
}

#[test]
fn a_bearer_token_identity_acts_for_its_did_through_the_token() {
    let device = RawIdentity::from_secp256r1(crypto::generate_secp256r1().unwrap()).unwrap();
    let token = String::from_utf8(
        identity::new_token(&device, Duration::from_secs(300), None, None).unwrap(),
    )
    .unwrap();
    let logical = RawIdentity::from_ed25519(crypto::generate_ed25519().unwrap()).unwrap();
    let logical_did = logical.did().unwrap().to_string();

    let did = cstr(&logical_did);
    let bearer = cstr(&token);
    let identity =
        registered(unsafe { defra_identity_new_from_bearer_token(did.as_ptr(), bearer.as_ptr()) });
    assert_eq!(ok(defra_identity_did(identity)), logical_did);
    assert_eq!(
        defra_core::signing::stored_bearer_token(&logical_did).as_deref(),
        Some(token.as_str())
    );

    let not_a_did = cstr("not-a-did");
    refused(unsafe { defra_identity_new_from_bearer_token(not_a_did.as_ptr(), bearer.as_ptr()) });
    refused(unsafe { defra_identity_new_from_bearer_token(did.as_ptr(), std::ptr::null()) });

    defra_identity_free(identity);
}
