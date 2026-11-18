//! Exporting some collections, and what an import refuses.

use std::ptr;

use serde_json::Value;

use super::{defra_backup_export, defra_backup_import};
use crate::test_support::{add_collection, close, cstr, err, node, ok, query};

/// A file in a fresh temporary directory holding `content`.
fn file_with(content: &str) -> (tempfile::TempDir, std::ffi::CString) {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("backup.json");
    std::fs::write(&file, content).unwrap();
    let path = cstr(&file.to_string_lossy());
    (directory, path)
}

#[test]
fn an_export_of_named_collections_holds_only_those() {
    let node = node();
    add_collection(
        node,
        "type User { name: String } type Address { city: String }",
    );
    query(
        node,
        r#"mutation { add_User(input: {name: "Alice"}) { _docID } }"#,
    );
    query(
        node,
        r#"mutation { add_Address(input: {city: "NYC"}) { _docID } }"#,
    );

    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("backup.json");
    let path = cstr(&file.to_string_lossy());
    let collections = cstr("Address");
    ok(unsafe { defra_backup_export(node, path.as_ptr(), collections.as_ptr(), ptr::null(), 0) });

    let exported: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert!(exported.get("Address").is_some(), "{exported}");
    assert!(exported.get("User").is_none(), "{exported}");
    let address = &exported["Address"][0];
    assert_eq!(address["city"], "NYC");
    assert!(address.get("_docID").is_some(), "{exported}");
    close(node);
}

#[test]
fn an_import_refuses_a_missing_file() {
    let node = node();
    let path = cstr("/nonexistent/path/file.json");
    let error = err(unsafe { defra_backup_import(node, path.as_ptr()) });
    assert!(error.contains("failed to open file"), "{error}");
    close(node);
}

#[test]
fn an_import_refuses_a_file_that_is_not_an_object_of_collections() {
    let node = node();
    add_collection(node, "type User { name: String }");
    for content in ["[]", "not json", r#"{"User": {"name": "x"}}"#] {
        let (_directory, path) = file_with(content);
        err(unsafe { defra_backup_import(node, path.as_ptr()) });
    }
    assert_eq!(
        query(node, "{ User { name } }")["data"]["User"],
        serde_json::json!([])
    );
    close(node);
}

#[test]
fn an_import_refuses_a_collection_the_node_does_not_have() {
    let node = node();
    add_collection(node, "type User { name: String }");
    let (_directory, path) = file_with(r#"{"Missing": [{"name": "x"}]}"#);
    err(unsafe { defra_backup_import(node, path.as_ptr()) });
    close(node);
}

#[test]
fn backup_refuses_a_transaction_handle() {
    let node = node();
    let txn = crate::txn::defra_transaction_create(node, 0);
    assert_eq!(txn.status, 0);
    let (_directory, path) = file_with("{}");
    err(unsafe { defra_backup_import(txn.txn_ptr, path.as_ptr()) });
    err(unsafe { defra_backup_export(txn.txn_ptr, path.as_ptr(), ptr::null(), ptr::null(), 0) });
    crate::txn::defra_transaction_discard(txn.txn_ptr);
    close(node);
}
