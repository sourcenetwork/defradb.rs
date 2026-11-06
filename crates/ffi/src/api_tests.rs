//! The smaller exports, each through its C entry point: version, views,
//! indexes, backups, subscriptions, actions and block verification.

use std::ptr;

use serde_json::{json, Value};

use crate::test_support::{add_collection, close, cstr, err, node, ok, ok_json, query, Named};
use crate::types::CollectionOptions;

#[test]
fn version_answers_as_text_or_json() {
    let short = ok(crate::version::defra_version_get(0, 0));
    assert!(short.starts_with("defradb "), "{short}");
    let full = ok(crate::version::defra_version_get(1, 0));
    assert!(full.contains("Go compat"), "{full}");
    let json = ok_json(crate::version::defra_version_get(0, 1));
    assert!(json["version"].is_string(), "{json}");
}

#[test]
fn a_view_answers_with_its_query_and_refreshes() {
    let node = node();
    add_collection(node, "type User { name: String }");
    query(
        node,
        r#"mutation { add_User(input: {name: "A"}) { _docID } }"#,
    );

    let view_query = cstr("User { name }");
    let sdl = cstr("type UserView @materialized(if: false) { name: String }");
    let created = ok_json(unsafe {
        crate::view::defra_view_add(node, view_query.as_ptr(), sdl.as_ptr(), ptr::null(), 0)
    });
    assert_eq!(created[0]["Name"], "UserView");
    assert_eq!(
        query(node, "{ UserView { name } }")["data"]["UserView"],
        json!([{"name": "A"}])
    );

    ok(unsafe { crate::view::defra_view_refresh(node, CollectionOptions::default(), 0) });
    close(node);
}

#[test]
fn an_index_is_created_listed_and_deleted() {
    let node = node();
    add_collection(node, "type User { name: String age: Int }");
    let users = Named::new("User");

    let name = cstr("by_age");
    let fields = cstr("age:DESC,name");
    let index = ok_json(unsafe {
        crate::index::defra_index_new(
            node,
            name.as_ptr(),
            fields.as_ptr(),
            0,
            ptr::null(),
            users.options(),
            0,
        )
    });
    assert_eq!(index["Name"], "by_age");
    assert_eq!(
        index["Fields"][0],
        json!({"Name": "age", "Descending": true})
    );

    let listed = ok_json(unsafe { crate::index::defra_index_list(node, users.options(), 0) });
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["Description"]["Name"] == "by_age"),
        "{listed}"
    );
    let all =
        ok_json(unsafe { crate::index::defra_index_list(node, CollectionOptions::default(), 0) });
    assert!(
        all.get("User").is_some(),
        "every collection's indexes, by name: {all}"
    );

    ok(unsafe { crate::index::defra_index_delete(node, name.as_ptr(), users.options(), 0) });
    let listed = ok_json(unsafe { crate::index::defra_index_list(node, users.options(), 0) });
    assert!(
        !listed
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["Description"]["Name"] == "by_age"),
        "{listed}"
    );

    let bad = cstr("age:SIDEWAYS");
    err(unsafe {
        crate::index::defra_index_new(
            node,
            name.as_ptr(),
            bad.as_ptr(),
            0,
            ptr::null(),
            users.options(),
            0,
        )
    });
    close(node);
}

#[test]
fn an_exported_backup_imports_into_another_node() {
    let source = node();
    add_collection(source, "type User { name: String }");
    query(
        source,
        r#"mutation { add_User(input: [{name: "A"}, {name: "B"}]) { _docID } }"#,
    );

    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("backup.json");
    let path = cstr(&file.to_string_lossy());
    ok(unsafe {
        crate::backup::defra_backup_export(source, path.as_ptr(), ptr::null(), ptr::null(), 1)
    });
    let exported: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(exported["User"].as_array().unwrap().len(), 2);

    let target = node();
    add_collection(target, "type User { name: String }");
    ok(unsafe { crate::backup::defra_backup_import(target, path.as_ptr()) });
    let names = query(target, "{ User(order: {name: ASC}) { name } }");
    assert_eq!(names["data"]["User"], json!([{"name": "A"}, {"name": "B"}]));

    let xml = cstr("xml");
    err(unsafe {
        crate::backup::defra_backup_export(source, path.as_ptr(), ptr::null(), xml.as_ptr(), 0)
    });
    close(source);
    close(target);
}

#[test]
fn a_subscription_answers_with_an_id_and_yields_updates() {
    let node = node();
    add_collection(node, "type User { name: String }");

    let request = cstr("subscription { User { name } }");
    let (status, id) = crate::test_support::take(unsafe {
        crate::query::defra_query_execute(node, request.as_ptr(), 0, ptr::null(), ptr::null())
    });
    assert_eq!(status, 2, "a subscription answers with status 2");
    let id = cstr(&id.unwrap());

    // Nothing yet: status 2 and no value.
    let (status, value) = crate::test_support::take(unsafe {
        crate::subscription::defra_subscription_poll(id.as_ptr())
    });
    assert_eq!((status, value), (2, Ok(String::new())));

    query(
        node,
        r#"mutation { add_User(input: {name: "A"}) { _docID } }"#,
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let update = loop {
        let (status, value) = crate::test_support::take(unsafe {
            crate::subscription::defra_subscription_poll(id.as_ptr())
        });
        if status == 0 {
            break value.unwrap();
        }
        assert!(std::time::Instant::now() < deadline, "no update arrived");
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let update: Value = serde_json::from_str(&update).unwrap();
    assert_eq!(update["data"]["User"][0]["name"], "A", "{update}");

    ok(unsafe { crate::subscription::defra_subscription_close(id.as_ptr()) });
    err(unsafe { crate::subscription::defra_subscription_poll(id.as_ptr()) });
    close(node);
}

#[test]
fn actions_list_as_json() {
    let node = node();
    let actions = ok_json(crate::action::defra_action_list(node, 0));
    assert!(actions.is_array() || actions.is_object(), "{actions}");
    close(node);
}

#[test]
fn a_signed_block_verifies_against_its_signer_only() {
    let identity = unsafe { crate::identities::defra_identity_new(ptr::null()) }.identity_ptr;
    let node = crate::test_support::node_with(crate::types::NodeInitOptions {
        identity_ptr: identity,
        enable_signing: 1,
        ..crate::test_support::local_options()
    });
    add_collection(node, "type User { name: String }");
    let created = query(
        node,
        r#"mutation { add_User(input: {name: "A"}) { _docID } }"#,
    );
    let doc_id = created["data"]["add_User"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string();
    let commits = query(
        node,
        &format!(r#"{{ _commits(docID: "{doc_id}") {{ cid }} }}"#),
    );
    let cid = cstr(commits["data"]["_commits"][0]["cid"].as_str().unwrap());

    let public_key = ok_json(crate::identities::defra_node_identity(node))["PublicKey"]
        .as_str()
        .unwrap()
        .to_string();
    let public_key = cstr(&public_key);
    let verified = ok(unsafe {
        crate::block::defra_block_verify(node, ptr::null(), public_key.as_ptr(), cid.as_ptr(), 0)
    });
    assert_eq!(verified, "Block's signature verified.");

    let stranger = unsafe { crate::identities::defra_identity_new(ptr::null()) }.identity_ptr;
    let other_node = crate::test_support::node_with(crate::types::NodeInitOptions {
        identity_ptr: stranger,
        ..crate::test_support::local_options()
    });
    let other_key = ok_json(crate::identities::defra_node_identity(other_node))["PublicKey"]
        .as_str()
        .unwrap()
        .to_string();
    let other_key = cstr(&other_key);
    err(unsafe {
        crate::block::defra_block_verify(node, ptr::null(), other_key.as_ptr(), cid.as_ptr(), 0)
    });

    close(other_node);
    close(node);
    crate::identities::defra_identity_free(identity);
    crate::identities::defra_identity_free(stranger);
}

#[test]
fn an_encrypted_index_is_created_listed_and_deleted() {
    let node = node();
    add_collection(node, "type User { name: String }");
    let collection = cstr("User");
    let field = cstr("name");

    let index = ok_json(unsafe {
        crate::encrypted_index::defra_encrypted_index_new(
            node,
            collection.as_ptr(),
            field.as_ptr(),
            ptr::null(),
            0,
        )
    });
    assert_eq!(index["FieldName"], "name");
    assert_eq!(index["Type"], "equality");

    let listed = ok_json(unsafe {
        crate::encrypted_index::defra_encrypted_index_list(node, collection.as_ptr(), 0)
    });
    assert_eq!(listed.as_array().map(Vec::len), Some(1), "{listed}");
    let all = ok_json(unsafe {
        crate::encrypted_index::defra_encrypted_index_list(node, ptr::null(), 0)
    });
    assert!(
        all.get("User").is_some(),
        "every collection's, by name: {all}"
    );

    ok(unsafe {
        crate::encrypted_index::defra_encrypted_index_delete(
            node,
            collection.as_ptr(),
            field.as_ptr(),
            0,
        )
    });
    let listed = ok_json(unsafe {
        crate::encrypted_index::defra_encrypted_index_list(node, collection.as_ptr(), 0)
    });
    assert_eq!(listed, json!([]));

    let unknown_type = cstr("range");
    err(unsafe {
        crate::encrypted_index::defra_encrypted_index_new(
            node,
            collection.as_ptr(),
            field.as_ptr(),
            unknown_type.as_ptr(),
            0,
        )
    });
    close(node);
}

#[test]
fn lenses_list_and_a_bad_lens_fails() {
    let node = node();
    let listed = ok_json(crate::lens::defra_lens_list(node, 0));
    assert!(
        listed.as_object().is_some_and(|l| l.is_empty()) || listed == json!([]),
        "{listed}"
    );

    let bad = cstr("not a lens");
    err(unsafe { crate::lens::defra_lens_add(node, 0, bad.as_ptr()) });
    close(node);
}
