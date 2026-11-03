//! Node access control's status, who may turn it back on, and what its
//! relationship calls refuse.

use std::ptr;

use super::*;
use crate::identities::{defra_identity_did, defra_identity_free, defra_identity_new};
use crate::test_support::{close, cstr, err, local_options, node_with, ok, ok_json};
use crate::types::NodeInitOptions;

fn identity() -> usize {
    let result = unsafe { defra_identity_new(ptr::null()) };
    assert_eq!(result.status, 0);
    result.identity_ptr
}

/// A node with node access control on, owned by `owner`.
fn owned_node(owner: usize) -> usize {
    node_with(NodeInitOptions {
        identity_ptr: owner,
        enable_node_acp: 1,
        ..local_options()
    })
}

#[test]
fn a_node_without_node_access_control_says_it_is_not_configured() {
    let node = node_with(local_options());
    let status = ok(defra_acp_nac_status(node, 0));
    assert!(status.contains("not configured"), "{status}");
    close(node);
}

#[test]
fn the_status_names_the_owner_and_follows_disable_and_re_enable() {
    let owner = identity();
    let node = owned_node(owner);
    let did = ok(defra_identity_did(owner));

    let status = ok(defra_acp_nac_status(node, owner));
    assert!(status.contains(r#""status":"enabled""#), "{status}");
    assert!(
        status.contains(&did),
        "the status names the owner: {status}"
    );

    ok(defra_acp_nac_disable(node, owner));
    let status = ok(defra_acp_nac_status(node, 0));
    assert!(status.contains("disabled temporarily"), "{status}");

    ok(defra_acp_nac_re_enable(node, owner));
    let status = ok_json(defra_acp_nac_status(node, owner));
    assert_eq!(status["status"], "enabled", "{status}");

    close(node);
    defra_identity_free(owner);
}

#[test]
fn re_enabling_checks_authorization_before_the_enabled_state() {
    let owner = identity();
    let stranger = identity();
    let node = owned_node(owner);

    for requestor in [0, stranger] {
        assert_eq!(
            err(defra_acp_nac_re_enable(node, requestor)),
            "not authorized to perform operation. Permission: re-enable-nac"
        );
    }
    assert_eq!(
        err(defra_acp_nac_re_enable(node, owner)),
        "node acp is already enabled"
    );

    close(node);
    defra_identity_free(owner);
    defra_identity_free(stranger);
}

#[test]
fn only_the_owner_may_disable_node_access_control() {
    let owner = identity();
    let stranger = identity();
    let node = owned_node(owner);

    err(defra_acp_nac_disable(node, stranger));
    err(defra_acp_nac_disable(node, 0));
    let status = ok_json(defra_acp_nac_status(node, owner));
    assert_eq!(status["status"], "enabled", "{status}");

    close(node);
    defra_identity_free(owner);
    defra_identity_free(stranger);
}

#[test]
fn a_relationship_needs_a_valid_actor_and_defaults_its_relation_to_admin() {
    let owner = identity();
    let node = owned_node(owner);
    let admin = cstr("admin");
    let invalid = cstr("invalid-did");

    err(unsafe { defra_acp_nac_relationship_add(node, owner, admin.as_ptr(), invalid.as_ptr()) });
    err(unsafe { defra_acp_nac_relationship_add(node, owner, admin.as_ptr(), ptr::null()) });
    let unknown = cstr("superuser");
    let actor = cstr(&ok(defra_identity_did(identity())));
    err(unsafe { defra_acp_nac_relationship_add(node, owner, unknown.as_ptr(), actor.as_ptr()) });
    let added = ok_json(unsafe {
        defra_acp_nac_relationship_add(node, owner, ptr::null(), actor.as_ptr())
    });
    assert_eq!(added["added"], true, "a null relation means admin: {added}");

    close(node);
    defra_identity_free(owner);
}

#[test]
fn a_relationship_change_is_refused_inside_a_transaction() {
    let owner = identity();
    let node = owned_node(owner);
    let txn = crate::txn::defra_transaction_create(node, 0);
    assert_eq!(txn.status, 0);

    let admin = cstr("admin");
    let actor = cstr(&ok(defra_identity_did(identity())));
    err(unsafe {
        defra_acp_nac_relationship_add(txn.txn_ptr, owner, admin.as_ptr(), actor.as_ptr())
    });

    crate::txn::defra_transaction_discard(txn.txn_ptr);
    close(node);
    defra_identity_free(owner);
}
