//! Exporting some collections, who may export, and what an import refuses.

use std::ptr;

use serde_json::Value;

use super::{defra_backup_export, defra_backup_import};
use crate::identities::{defra_identity_free, defra_identity_new};
use crate::test_support::{
    add_collection, close, cstr, err, local_options, node, node_with, ok, ok_json, query, query_as,
};
use crate::types::NodeInitOptions;

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
    ok(unsafe {
        defra_backup_export(node, path.as_ptr(), collections.as_ptr(), ptr::null(), 0, 0)
    });

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
    let error = err(unsafe { defra_backup_import(node, path.as_ptr(), 0) });
    assert!(error.contains("failed to open file"), "{error}");
    close(node);
}

#[test]
fn an_import_refuses_a_file_that_is_not_an_object_of_collections() {
    let node = node();
    add_collection(node, "type User { name: String }");
    for content in ["[]", "not json", r#"{"User": {"name": "x"}}"#] {
        let (_directory, path) = file_with(content);
        err(unsafe { defra_backup_import(node, path.as_ptr(), 0) });
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
    err(unsafe { defra_backup_import(node, path.as_ptr(), 0) });
    close(node);
}

#[test]
fn backup_refuses_a_transaction_handle() {
    let node = node();
    let txn = crate::txn::defra_transaction_create(node, 0);
    assert_eq!(txn.status, 0);
    let (_directory, path) = file_with("{}");
    err(unsafe { defra_backup_import(txn.txn_ptr, path.as_ptr(), 0) });
    err(unsafe { defra_backup_export(txn.txn_ptr, path.as_ptr(), ptr::null(), ptr::null(), 0, 0) });
    crate::txn::defra_transaction_discard(txn.txn_ptr);
    close(node);
}

#[test]
fn node_access_control_refuses_an_export_to_a_stranger() {
    let identity = || {
        let result = unsafe { defra_identity_new(ptr::null()) };
        assert_eq!(result.status, 0);
        result.identity_ptr
    };
    let (owner, stranger) = (identity(), identity());
    let node = node_with(NodeInitOptions {
        identity_ptr: owner,
        enable_node_acp: 1,
        ..local_options()
    });
    let sdl = cstr("type User { name: String }");
    ok_json(unsafe { crate::collection::defra_collection_add(node, sdl.as_ptr(), owner) });
    query_as(
        node,
        r#"mutation { add_User(input: {name: "Alice"}) { _docID } }"#,
        owner,
    );
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("backup.json");
    let path = cstr(&file.to_string_lossy());

    let export_as = |caller| unsafe {
        defra_backup_export(node, path.as_ptr(), ptr::null(), ptr::null(), 0, caller)
    };
    assert_eq!(
        err(export_as(stranger)),
        "not authorized to perform operation. Permission: read-document"
    );
    assert!(!file.exists(), "a refused export writes no file");

    ok(export_as(owner));
    let exported: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(exported["User"][0]["name"], "Alice", "{exported}");

    close(node);
    defra_identity_free(owner);
    defra_identity_free(stranger);
}
