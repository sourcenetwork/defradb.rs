//! How long an identity's signing key stays in the process-wide signing
//! store: while a handle or a node holds it, and no longer.

use std::ptr;

use defra_core::signing::get_identity;

use crate::acp::defra_node_set_identity;
use crate::identities::*;
use crate::test_support::{close, cstr, local_options, node_with, ok};
use crate::types::NodeInitOptions;

fn new_identity() -> usize {
    let result = unsafe { defra_identity_new(ptr::null()) };
    assert_eq!(result.status, 0, "identity creation should succeed");
    result.identity_ptr
}

fn did(identity: usize) -> String {
    ok(defra_identity_did(identity))
}

#[test]
fn freeing_the_last_handle_removes_the_key() {
    let first = new_identity();
    let key = cstr(&ok(defra_identity_export_private_key(first)));
    let second = unsafe { defra_identity_new_from_private_key(key.as_ptr()) }.identity_ptr;
    let did = did(first);
    assert_eq!(did, self::did(second), "both handles name one DID");

    defra_identity_free(first);
    assert!(
        get_identity(&did).is_some(),
        "the second handle still holds it"
    );
    defra_identity_free(second);
    assert!(get_identity(&did).is_none(), "no handle holds it");
}

#[test]
fn closing_one_node_leaves_another_nodes_identities_alone() {
    let a = node_with(local_options());
    let b = node_with(local_options());
    let identity = new_identity();
    let did = did(identity);

    close(a);
    assert!(
        get_identity(&did).is_some(),
        "the handle still holds its key"
    );
    ok(defra_node_set_identity(b, identity));
    assert_eq!(
        crate::state::NODES
            .get(b, |state| state.identity_did())
            .flatten(),
        Some(did.clone())
    );

    close(b);
    defra_identity_free(identity);
    assert!(get_identity(&did).is_none());
}

#[test]
fn a_node_keeps_its_identity_after_the_handle_is_freed() {
    let identity = new_identity();
    let did = did(identity);
    let node = node_with(NodeInitOptions {
        identity_ptr: identity,
        enable_signing: 1,
        ..local_options()
    });

    defra_identity_free(identity);
    assert!(get_identity(&did).is_some(), "the node holds its identity");

    close(node);
    assert!(get_identity(&did).is_none(), "nothing holds it any more");
}

#[test]
fn replacing_a_node_identity_gives_back_the_old_hold() {
    let node = node_with(local_options());
    let identity = new_identity();
    let did = did(identity);
    ok(defra_node_set_identity(node, identity));
    defra_identity_free(identity);
    assert!(get_identity(&did).is_some(), "the node holds it");

    ok(defra_node_set_identity(node, 0));
    assert!(get_identity(&did).is_none(), "the node gave it back");
    close(node);
}
