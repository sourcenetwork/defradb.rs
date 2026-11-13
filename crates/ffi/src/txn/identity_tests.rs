//! A transaction begun through the C API, which carries no identity, serves
//! every later call whatever identity it acts as, as Go's does (#2335).

use crate::collection::defra_collection_describe;
use crate::identities::{defra_identity_free, defra_identity_new};
use crate::test_support::{add_collection, close, node, ok, ok_json, query_as};
use crate::txn::{defra_transaction_commit, defra_transaction_create};
use crate::types::{defra_new_identity_result_free, CollectionOptions};

/// A new identity's handle.
fn identity() -> usize {
    let mut result = unsafe { defra_identity_new(std::ptr::null()) };
    assert_eq!(result.status, 0, "defra_identity_new must succeed");
    let handle = result.identity_ptr;
    unsafe { defra_new_identity_result_free(&mut result) };
    handle
}

#[test]
fn a_transaction_serves_calls_that_act_as_an_identity() {
    let node = node();
    add_collection(node, "type TxnIdentity { value: Int }");

    let txn = defra_transaction_create(node, 0);
    assert_eq!(txn.status, 0, "defra_transaction_create must succeed");
    let txn = txn.txn_ptr;

    for caller in [identity(), identity()] {
        let collections = ok_json(unsafe {
            defra_collection_describe(txn, CollectionOptions::default(), caller)
        });
        assert!(
            collections.to_string().contains("TxnIdentity"),
            "{collections}"
        );

        let read = query_as(txn, "{ TxnIdentity { value } }", caller);
        assert!(read.get("errors").is_none(), "{read}");
        defra_identity_free(caller);
    }

    ok(defra_transaction_commit(txn));
    close(node);
}
