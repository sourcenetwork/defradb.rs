use std::ptr;

use serde_json::json;

use super::*;
use crate::identities::{defra_identity_did, defra_identity_free, defra_identity_new};
use crate::test_support::{close, cstr, err, local_options, node_with, ok, ok_json, query_as};
use crate::types::NodeInitOptions;

fn identity() -> usize {
    let result = unsafe { defra_identity_new(ptr::null()) };
    assert_eq!(result.status, 0);
    result.identity_ptr
}

const POLICY: &str = r#"
name: Viewer Policy
resources:
  - name: users
    relations:
      - name: viewer
      - name: editor
      - name: remover
    permissions:
      - name: read
        expr: viewer
      - name: update
        expr: editor
      - name: delete
        expr: remover
"#;

#[test]
fn a_document_policy_hides_documents_until_a_relationship_grants_them() {
    let owner = identity();
    let reader = identity();
    let node = node_with(local_options());

    let policy = cstr(POLICY);
    let added = ok_json(unsafe { defra_acp_dac_policy_add(node, owner, policy.as_ptr()) });
    let policy_id = added["PolicyID"]
        .as_str()
        .expect("the policy has an ID")
        .to_string();

    let sdl = cstr(&format!(
        r#"type User @policy(id: "{policy_id}", resource: "users") {{ name: String }}"#
    ));
    ok(unsafe { crate::collection::defra_collection_add(node, sdl.as_ptr(), owner) });
    let created = query_as(
        node,
        r#"mutation { add_User(input: {name: "Alice"}) { _docID } }"#,
        owner,
    );
    let doc_id = created["data"]["add_User"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string();

    let read = |who| query_as(node, "{ User { name } }", who)["data"]["User"].clone();
    assert_eq!(read(owner), json!([{"name": "Alice"}]));
    assert_eq!(read(reader), json!([]));

    let collection = cstr("User");
    let doc = cstr(&doc_id);
    let relation = cstr("viewer");
    let actor = cstr(&ok(defra_identity_did(reader)));
    ok(unsafe {
        defra_acp_dac_relationship_add(
            node,
            owner,
            collection.as_ptr(),
            doc.as_ptr(),
            relation.as_ptr(),
            actor.as_ptr(),
        )
    });
    assert_eq!(read(reader), json!([{"name": "Alice"}]));

    ok(unsafe {
        defra_acp_dac_relationship_delete(
            node,
            owner,
            collection.as_ptr(),
            doc.as_ptr(),
            relation.as_ptr(),
            actor.as_ptr(),
        )
    });
    assert_eq!(read(reader), json!([]));

    // Without an identity, no one creates the policy.
    err(unsafe { defra_acp_dac_policy_add(node, 0, policy.as_ptr()) });

    close(node);
    defra_identity_free(owner);
    defra_identity_free(reader);
}

#[test]
fn node_access_control_enabled_at_startup_admits_only_its_owner_and_grantees() {
    let owner = identity();
    let stranger = identity();
    let node = node_with(NodeInitOptions {
        identity_ptr: owner,
        enable_node_acp: 1,
        ..local_options()
    });

    let status = ok_json(defra_acp_nac_status(node, owner));
    assert_eq!(status["status"], "enabled", "{status}");

    let sdl = cstr("type User { name: String }");
    err(unsafe { crate::collection::defra_collection_add(node, sdl.as_ptr(), stranger) });
    ok(unsafe { crate::collection::defra_collection_add(node, sdl.as_ptr(), owner) });

    let relation = cstr("admin");
    let actor = cstr(&ok(defra_identity_did(stranger)));
    ok(unsafe { defra_acp_nac_relationship_add(node, owner, relation.as_ptr(), actor.as_ptr()) });
    let other = cstr("type Post { title: String }");
    ok(unsafe { crate::collection::defra_collection_add(node, other.as_ptr(), stranger) });

    ok(unsafe {
        defra_acp_nac_relationship_delete(node, owner, relation.as_ptr(), actor.as_ptr())
    });
    let third = cstr("type Tag { label: String }");
    err(unsafe { crate::collection::defra_collection_add(node, third.as_ptr(), stranger) });

    ok(defra_acp_nac_disable(node, owner));
    ok(unsafe { crate::collection::defra_collection_add(node, third.as_ptr(), stranger) });
    ok(defra_acp_nac_re_enable(node, owner));
    let fourth = cstr("type Note { text: String }");
    err(unsafe { crate::collection::defra_collection_add(node, fourth.as_ptr(), stranger) });

    close(node);
    defra_identity_free(owner);
    defra_identity_free(stranger);
}

#[test]
fn a_document_relationship_needs_a_collection_the_node_has() {
    let owner = identity();
    let node = node_with(local_options());
    let collection = cstr("Missing");
    let doc = cstr("bae-00000000-0000-0000-0000-000000000000");
    let relation = cstr("viewer");
    let actor = cstr(&ok(defra_identity_did(identity())));
    err(unsafe {
        defra_acp_dac_relationship_add(
            node,
            owner,
            collection.as_ptr(),
            doc.as_ptr(),
            relation.as_ptr(),
            actor.as_ptr(),
        )
    });
    close(node);
    defra_identity_free(owner);
}
