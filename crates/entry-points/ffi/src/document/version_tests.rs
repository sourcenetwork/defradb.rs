//! Document calls through a selected collection version, inactive ones
//! included, as Go runs a document call on the selected collection
//! (`proofs/tla/VersionPin.tla`).

use std::ffi::CString;
use std::ptr;

use serde_json::Value;

use super::{defra_document_add, defra_document_get};
use crate::collection::defra_collection_patch;
use crate::test_support::{add_collection, close, cstr, node, ok, ok_json, query};
use crate::txn::{defra_transaction_commit, defra_transaction_create};
use crate::types::{defra_result_free, CollectionOptions};

/// A node with `User { name }` at its first version, then a second, active
/// version that adds `age`. Answers the node and the first version's ID.
fn two_versions() -> (usize, String) {
    let node = node();
    let created = add_collection(node, "type User { name: String }");
    let first = created[0]["VersionID"].as_str().unwrap().to_owned();
    let patch = cstr(
        r#"[{"op": "add", "path": "/User/Fields/-", "value": {"Name": "age", "Kind": "Int"}}]"#,
    );
    ok(unsafe { defra_collection_patch(node, patch.as_ptr(), ptr::null(), 0) });
    (node, first)
}

/// Options selecting collection version `version`.
fn at_version(version: &CString) -> CollectionOptions {
    CollectionOptions {
        version: version.as_ptr(),
        get_inactive: 1,
        ..CollectionOptions::default()
    }
}

/// Options selecting the active `User`.
fn active_user(name: &CString) -> CollectionOptions {
    CollectionOptions {
        name: name.as_ptr(),
        ..CollectionOptions::default()
    }
}

/// Adds `{"name": name}` on `handle` (a node or a transaction) through
/// `options`, answering its document ID.
fn add(handle: usize, name: &str, options: CollectionOptions) -> String {
    let json = cstr(&format!(r#"{{"name": "{name}"}}"#));
    let ids =
        ok_json(unsafe { defra_document_add(handle, json.as_ptr(), 0, ptr::null(), options, 0) });
    ids[0].as_str().expect("add answers with an ID").to_owned()
}

/// The collection version every commit of document `doc_id` records.
fn commit_versions(node: usize, doc_id: &str) -> Vec<String> {
    let commits = query(
        node,
        &format!(r#"{{ _commits(docID: "{doc_id}") {{ collectionVersionId }} }}"#),
    );
    commits["data"]["_commits"]
        .as_array()
        .unwrap_or_else(|| panic!("commits answer: {commits}"))
        .iter()
        .map(|commit| commit["collectionVersionId"].as_str().unwrap().to_owned())
        .collect()
}

/// A document added through an inactive version is written at that version:
/// every one of its commits records the version it was added through.
#[test]
fn a_document_added_through_an_old_version_is_recorded_at_it() {
    let (node, first) = two_versions();
    let first_id = cstr(&first);

    let doc = add(node, "Old", at_version(&first_id));
    let versions = commit_versions(node, &doc);
    assert!(!versions.is_empty(), "the document has commits");
    assert!(
        versions.iter().all(|version| *version == first),
        "written at {first}: {versions:?}"
    );
    close(node);
}

/// A get reads through the version it selects: through an old version it
/// answers a field the active version removed, which a read through the
/// active version could not even ask for; through the active version it
/// answers without that field.
#[test]
fn a_get_answers_the_fields_of_the_version_it_reads_through() {
    let node = node();
    let created = add_collection(node, "type User { name: String nick: String }");
    let first = created[0]["VersionID"].as_str().unwrap().to_owned();
    let first_id = cstr(&first);
    let json = cstr(r#"{"name": "Reader", "nick": "R"}"#);
    let ids = ok_json(unsafe {
        defra_document_add(
            node,
            json.as_ptr(),
            0,
            ptr::null(),
            at_version(&first_id),
            0,
        )
    });
    let doc_id = cstr(ids[0].as_str().unwrap());
    // The second, active version drops `nick`.
    let patch = cstr(r#"[{"op": "remove", "path": "/User/Fields/2"}]"#);
    ok(unsafe { defra_collection_patch(node, patch.as_ptr(), ptr::null(), 0) });

    let old: Value = serde_json::from_str(&ok(unsafe {
        defra_document_get(node, doc_id.as_ptr(), 0, at_version(&first_id), 0)
    }))
    .unwrap();
    assert_eq!(old["nick"], "R", "read through the old version: {old}");

    let user = cstr("User");
    let active: Value = serde_json::from_str(&ok(unsafe {
        defra_document_get(node, doc_id.as_ptr(), 0, active_user(&user), 0)
    }))
    .unwrap();
    assert_eq!(active["name"], "Reader", "{active}");
    assert!(
        active.get("nick").is_none(),
        "the active version has no nick: {active}"
    );
    close(node);
}

/// In an explicit transaction, a call through an old version pins it for its
/// own turn only: the next call by name is written at the active version.
#[test]
fn a_selected_version_does_not_outlive_its_call_in_a_transaction() {
    let (node, first) = two_versions();
    let first_id = cstr(&first);
    let user = cstr("User");
    let txn = defra_transaction_create(node, 0).txn_ptr;

    let old = add(txn, "Old", at_version(&first_id));
    let current = add(txn, "Current", active_user(&user));
    let mut committed = defra_transaction_commit(txn);
    assert_eq!(committed.status, 0);
    unsafe { defra_result_free(&mut committed) };

    assert!(commit_versions(node, &old)
        .iter()
        .all(|version| *version == first));
    assert!(
        commit_versions(node, &current)
            .iter()
            .all(|version| *version != first),
        "the call by name was written at the active version"
    );
    close(node);
}
