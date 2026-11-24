use std::ptr;

use crate::identities::*;
use crate::test_support::{close, cstr, err, local_options, node_with, ok, ok_json};
use crate::types::{DefraNewIdentityResult, NodeInitOptions};

fn handle(result: DefraNewIdentityResult) -> usize {
    assert_eq!(result.status, 0, "identity creation should succeed");
    assert_ne!(result.identity_ptr, 0);
    result.identity_ptr
}

#[test]
fn every_key_type_makes_an_identity() {
    for key_type in ["", "secp256k1", "secp256r1", "ed25519"] {
        let key_type = cstr(key_type);
        let identity = handle(unsafe { defra_identity_new(key_type.as_ptr()) });
        assert!(ok(defra_identity_did(identity)).starts_with("did:key:"));
        defra_identity_free(identity);
    }
    let unknown = cstr("rsa");
    let mut result = unsafe { defra_identity_new(unknown.as_ptr()) };
    assert_eq!(result.status, 1);
    unsafe { crate::types::defra_new_identity_result_free(&mut result) };
}

#[test]
fn an_exported_private_key_imports_the_same_identity() {
    let identity = handle(unsafe { defra_identity_new(ptr::null()) });
    let key = ok(defra_identity_export_private_key(identity));
    assert_eq!(key.len(), 64, "a secp256k1 key is 32 bytes of hex");

    let key = cstr(&key);
    let imported = handle(unsafe { defra_identity_new_from_private_key(key.as_ptr()) });
    assert_eq!(
        ok(defra_identity_did(identity)),
        ok(defra_identity_did(imported))
    );

    defra_identity_free(identity);
    defra_identity_free(imported);
}

#[test]
fn every_key_type_exports_and_imports_back() {
    for name in ["secp256k1", "secp256r1", "ed25519"] {
        let key_type = cstr(name);
        let identity = handle(unsafe { defra_identity_new(key_type.as_ptr()) });
        let key = cstr(&ok(defra_identity_export_private_key(identity)));

        let imported = handle(unsafe {
            defra_identity_new_from_typed_private_key(key_type.as_ptr(), key.as_ptr())
        });
        assert_eq!(
            ok(defra_identity_did(identity)),
            ok(defra_identity_did(imported)),
            "{name} imports as the identity it was exported from"
        );

        defra_identity_free(identity);
        defra_identity_free(imported);
    }
}

#[test]
fn a_typed_import_refuses_an_unknown_key_type() {
    let identity = handle(unsafe { defra_identity_new(ptr::null()) });
    let key = cstr(&ok(defra_identity_export_private_key(identity)));
    let rsa = cstr("rsa");
    let mut result =
        unsafe { defra_identity_new_from_typed_private_key(rsa.as_ptr(), key.as_ptr()) };
    assert_eq!(result.status, 1);
    unsafe { crate::types::defra_new_identity_result_free(&mut result) };
    defra_identity_free(identity);
}

#[test]
fn a_bad_private_key_fails() {
    for key in ["zz", "", "abcd"] {
        let key = cstr(key);
        let mut result = unsafe { defra_identity_new_from_private_key(key.as_ptr()) };
        assert_eq!(result.status, 1, "{key:?} must not import");
        unsafe { crate::types::defra_new_identity_result_free(&mut result) };
    }
}

#[test]
fn a_freed_identity_is_gone() {
    let identity = handle(unsafe { defra_identity_new(ptr::null()) });
    defra_identity_free(identity);
    err(defra_identity_did(identity));
    err(defra_identity_export_private_key(identity));
    // Freeing again, or the null handle, does nothing.
    defra_identity_free(identity);
    defra_identity_free(0);
    err(defra_identity_export_private_key(0));
}

#[test]
fn a_node_takes_its_identity_from_the_handle() {
    let identity = handle(unsafe { defra_identity_new(ptr::null()) });
    let node = node_with(NodeInitOptions {
        identity_ptr: identity,
        ..local_options()
    });
    let node_identity = ok_json(defra_node_identity(node));
    assert_eq!(
        node_identity["DID"],
        ok(defra_identity_did(identity)).as_str()
    );
    assert!(node_identity["PublicKey"]
        .as_str()
        .is_some_and(|key| !key.is_empty()));
    close(node);
    defra_identity_free(identity);
}

#[test]
fn a_node_without_an_identity_says_so() {
    let node = node_with(local_options());
    assert_eq!(
        ok(defra_node_identity(node)),
        "Node has no identity assigned to it."
    );
    close(node);
}
