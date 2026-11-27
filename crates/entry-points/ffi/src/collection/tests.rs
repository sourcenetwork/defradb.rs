use std::ptr;

use serde_json::Value;

use super::*;
use crate::test_support::{add_collection, close, cstr, err, node, ok, ok_json, query, Named};
use crate::txn::{defra_transaction_commit, defra_transaction_create};
use crate::types::CollectionOptions;

fn describe(handle: usize, options: CollectionOptions) -> Vec<Value> {
    match ok_json(unsafe { defra_collection_describe(handle, options, 0) }) {
        Value::Array(versions) => versions,
        other => panic!("describe answers with an array: {other}"),
    }
}

fn names(versions: &[Value]) -> Vec<&str> {
    versions
        .iter()
        .map(|v| v["Name"].as_str().unwrap())
        .collect()
}

#[test]
fn added_collections_answer_with_their_versions() {
    let node = node();
    let created = add_collection(
        node,
        "type User { name: String } type Post { title: String }",
    );
    let mut created = created
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["Name"].as_str().unwrap())
        .collect::<Vec<_>>();
    created.sort();
    assert_eq!(created, ["Post", "User"]);
    close(node);
}

#[test]
fn describe_selects_by_name_and_version() {
    let node = node();
    add_collection(
        node,
        "type User { name: String } type Post { title: String }",
    );

    assert_eq!(
        names(&describe(node, CollectionOptions::default())),
        ["Post", "User"]
    );

    let users = Named::new("User");
    let user = describe(node, users.options());
    assert_eq!(names(&user), ["User"]);

    let version = cstr(user[0]["VersionID"].as_str().unwrap());
    let by_version = describe(
        node,
        CollectionOptions {
            version: version.as_ptr(),
            ..CollectionOptions::default()
        },
    );
    assert_eq!(names(&by_version), ["User"]);

    // An unknown name selects nothing; an unknown version is not found.
    assert!(describe(node, Named::new("Nope").options()).is_empty());
    let unknown = cstr("bafkunknown");
    err(unsafe {
        defra_collection_describe(
            node,
            CollectionOptions {
                version: unknown.as_ptr(),
                ..CollectionOptions::default()
            },
            0,
        )
    });
    close(node);
}

#[test]
fn a_patch_adds_a_field() {
    let node = node();
    add_collection(node, "type User { name: String }");

    let patch = cstr(
        r#"[{"op": "add", "path": "/User/Fields/-", "value": {"Name": "age", "Kind": "Int"}}]"#,
    );
    ok(unsafe { defra_collection_patch(node, patch.as_ptr(), ptr::null(), 0) });

    query(
        node,
        r#"mutation { add_User(input: {name: "A", age: 3}) { _docID } }"#,
    );
    let users = query(node, "{ User { age } }");
    assert_eq!(users["data"]["User"][0]["age"], 3);
    close(node);
}

#[test]
fn set_active_activates_a_version() {
    let node = node();
    let created = add_collection(node, "type User { name: String }");
    let version = cstr(created[0]["VersionID"].as_str().unwrap());
    ok(unsafe {
        defra_collection_set_active(
            node,
            CollectionOptions {
                version: version.as_ptr(),
                ..CollectionOptions::default()
            },
            0,
        )
    });
    close(node);
}

#[test]
fn truncate_deletes_documents_and_keeps_the_collection() {
    let node = node();
    add_collection(node, "type User { name: String }");
    query(
        node,
        r#"mutation { add_User(input: [{name: "A"}, {name: "B"}]) { _docID } }"#,
    );

    let users = Named::new("User");
    let filter = cstr(r#"{"name": {"_eq": "A"}}"#);
    ok(unsafe { defra_collection_truncate_with_filter(node, users.options(), 0, filter.as_ptr()) });
    assert_eq!(
        query(node, "{ User { name } }")["data"]["User"][0]["name"],
        "B"
    );

    ok(unsafe { defra_collection_truncate(node, users.options(), 0) });
    assert_eq!(
        query(node, "{ User { name } }")["data"]["User"],
        serde_json::json!([])
    );

    // A JSON null filter, or none, must not truncate everything.
    let null = cstr("null");
    err(unsafe { defra_collection_truncate_with_filter(node, users.options(), 0, null.as_ptr()) });
    err(unsafe { defra_collection_truncate_with_filter(node, users.options(), 0, ptr::null()) });
    close(node);
}

#[test]
fn delete_removes_collections() {
    let node = node();
    add_collection(
        node,
        "type User { name: String } type Post { title: String }",
    );
    let names_arg = cstr("User,Post");
    ok(unsafe { defra_collection_delete(node, names_arg.as_ptr(), 0, 0) });
    assert!(describe(node, CollectionOptions::default()).is_empty());
    close(node);
}

#[test]
fn a_collection_added_in_a_transaction_is_seen_only_there_until_commit() {
    let node = node();
    let txn = defra_transaction_create(node, 0);
    assert_eq!(txn.status, 0);

    add_collection(txn.txn_ptr, "type User { name: String }");
    assert_eq!(
        names(&describe(txn.txn_ptr, CollectionOptions::default())),
        ["User"]
    );
    assert!(describe(node, CollectionOptions::default()).is_empty());

    ok(defra_transaction_commit(txn.txn_ptr));
    assert_eq!(
        names(&describe(node, CollectionOptions::default())),
        ["User"]
    );
    close(node);
}
