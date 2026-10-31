use crate::node::{new_node, node_close};
use crate::schema::add_schema;
use crate::txn::{begin_txn, commit_txn, rollback_txn};
use crate::types::NodeInitOptions;
use std::ffi::CString;

#[test]
fn test_transaction_lifecycle() {
    assert!(crate::runtime::init_runtime());

    // Create node
    let options = NodeInitOptions::default();
    let result = new_node(options);
    assert_eq!(result.status, 0);
    let node = result.node_ptr;

    // Add schema
    let sdl = CString::new("type TxnTest { value: Int }").unwrap();
    let result = unsafe { add_schema(node, std::ptr::null(), sdl.as_ptr()) };
    assert_eq!(result.status, 0);

    // Begin transaction
    let result = begin_txn(node, 0);
    assert_eq!(result.status, 0, "begin_txn should succeed");
    assert!(!result.txn_id.is_null());

    let txn_id = unsafe { std::ffi::CStr::from_ptr(result.txn_id).to_string_lossy() };
    assert!(!txn_id.is_empty());

    // Execute in transaction
    let mutation =
        CString::new(r#"mutation { add_TxnTest(input: {value: 42}) { _docID value } }"#).unwrap();
    let txn_id_cstr = CString::new(txn_id.as_ref()).unwrap();
    let result = unsafe {
        crate::query::exec_request(
            node,
            txn_id_cstr.as_ptr(),
            std::ptr::null(),
            mutation.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert_eq!(
        result.status, 0,
        "exec_request in a transaction should succeed"
    );

    let response: serde_json::Value = unsafe {
        serde_json::from_str(
            std::ffi::CStr::from_ptr(result.value)
                .to_str()
                .expect("mutation response should be UTF-8"),
        )
        .expect("mutation response should be JSON")
    };
    let doc_id = response["data"]["add_TxnTest"][0]["_docID"]
        .as_str()
        .expect("mutation should return a document ID");
    unsafe { crate::types::defra_free_string(result.value) };

    let commits_query =
        CString::new(format!(r#"{{ _commits(docID: "{doc_id}") {{ cid }} }}"#)).unwrap();
    let result = unsafe {
        crate::query::exec_request(
            node,
            txn_id_cstr.as_ptr(),
            std::ptr::null(),
            commits_query.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert_eq!(result.status, 0, "transaction commit query should succeed");
    let response: serde_json::Value = unsafe {
        serde_json::from_str(
            std::ffi::CStr::from_ptr(result.value)
                .to_str()
                .expect("commit response should be UTF-8"),
        )
        .expect("commit response should be JSON")
    };
    assert!(
        !response["data"]["_commits"]
            .as_array()
            .expect("commit response should be an array")
            .is_empty(),
        "transaction should expose its uncommitted commits"
    );
    unsafe { crate::types::defra_free_string(result.value) };

    // Commit transaction
    let result = unsafe { commit_txn(node, txn_id_cstr.as_ptr()) };
    assert_eq!(result.status, 0, "commit_txn should succeed");

    // Cleanup
    node_close(node);
}

#[test]
fn test_transaction_rollback() {
    assert!(crate::runtime::init_runtime());

    let options = NodeInitOptions::default();
    let result = new_node(options);
    assert_eq!(result.status, 0);
    let node = result.node_ptr;

    let sdl = CString::new("type RollbackTest { value: Int }").unwrap();
    let result = unsafe { add_schema(node, std::ptr::null(), sdl.as_ptr()) };
    assert_eq!(result.status, 0);

    // Begin transaction
    let result = begin_txn(node, 0);
    assert_eq!(result.status, 0);
    let txn_id = unsafe { std::ffi::CStr::from_ptr(result.txn_id).to_string_lossy() };
    let txn_id_cstr = CString::new(txn_id.as_ref()).unwrap();

    // Execute in transaction
    let mutation =
        CString::new(r#"mutation { add_RollbackTest(input: {value: 99}) { _docID } }"#).unwrap();
    let result = unsafe {
        crate::query::exec_request(
            node,
            txn_id_cstr.as_ptr(),
            std::ptr::null(),
            mutation.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert_eq!(result.status, 0);

    // Rollback transaction
    let result = unsafe { rollback_txn(node, txn_id_cstr.as_ptr()) };
    assert_eq!(result.status, 0, "rollback_txn should succeed");

    node_close(node);
}

#[test]
fn test_readonly_transaction() {
    assert!(crate::runtime::init_runtime());

    let options = NodeInitOptions::default();
    let result = new_node(options);
    assert_eq!(result.status, 0);
    let node = result.node_ptr;

    let sdl = CString::new("type ReadOnlyTest { value: Int }").unwrap();
    let result = unsafe { add_schema(node, std::ptr::null(), sdl.as_ptr()) };
    assert_eq!(result.status, 0);

    // Begin readonly transaction
    let result = begin_txn(node, 1); // readonly = true
    assert_eq!(result.status, 0, "begin readonly txn should succeed");

    let txn_id = unsafe { std::ffi::CStr::from_ptr(result.txn_id).to_string_lossy() };
    let txn_id_cstr = CString::new(txn_id.as_ref()).unwrap();

    // Query should work in readonly transaction
    let query = CString::new("{ ReadOnlyTest { value } }").unwrap();
    let result = unsafe {
        crate::query::exec_request(
            node,
            txn_id_cstr.as_ptr(),
            std::ptr::null(),
            query.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert_eq!(result.status, 0, "query in readonly txn should succeed");

    // Commit readonly transaction
    let result = unsafe { commit_txn(node, txn_id_cstr.as_ptr()) };
    assert_eq!(result.status, 0);

    node_close(node);
}
