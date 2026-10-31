//! A transaction begun through the C API, which carries no identity, serves
//! every later call whatever identity it acts as, as Go's does (#2335).

use crate::node::{new_node, node_close};
use crate::schema::{add_schema, get_collections_in_txn};
use crate::txn::{begin_txn, commit_txn, exec_request_in_txn};
use crate::types::{defra_free_string, NodeInitOptions};
use std::ffi::{CStr, CString};
use std::ptr;

#[test]
fn a_transaction_serves_calls_that_act_as_an_identity() {
    assert!(crate::runtime::init_runtime());
    let node = new_node(NodeInitOptions::default());
    assert_eq!(node.status, 0);
    let node = node.node_ptr;

    let sdl = CString::new("type TxnIdentity { value: Int }").unwrap();
    assert_eq!(
        unsafe { add_schema(node, ptr::null(), sdl.as_ptr()) }.status,
        0
    );

    let txn = begin_txn(node, 0);
    assert_eq!(txn.status, 0, "begin_txn must succeed");
    let txn_id = unsafe { CStr::from_ptr(txn.txn_id).to_string_lossy().into_owned() };
    unsafe { defra_free_string(txn.txn_id) };
    let txn_id = CString::new(txn_id).unwrap();

    for did in ["did:key:alice", "did:key:bob"] {
        let did = CString::new(did).unwrap();

        let collections = unsafe { get_collections_in_txn(node, txn_id.as_ptr(), did.as_ptr()) };
        assert_eq!(
            collections.status, 0,
            "get_collections_in_txn must reach the transaction"
        );
        let json = unsafe {
            CStr::from_ptr(collections.value)
                .to_string_lossy()
                .into_owned()
        };
        unsafe { defra_free_string(collections.value) };
        assert!(json.contains("TxnIdentity"), "{json}");

        let query = CString::new("{ TxnIdentity { value } }").unwrap();
        let read = unsafe {
            exec_request_in_txn(
                node,
                txn_id.as_ptr(),
                did.as_ptr(),
                query.as_ptr(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
            )
        };
        assert_eq!(
            read.status, 0,
            "exec_request_in_txn must reach the transaction"
        );
        let json = unsafe { CStr::from_ptr(read.value).to_string_lossy().into_owned() };
        unsafe { defra_free_string(read.value) };
        assert!(!json.contains("not found"), "{json}");
    }

    assert_eq!(unsafe { commit_txn(node, txn_id.as_ptr()) }.status, 0);
    node_close(node);
}
