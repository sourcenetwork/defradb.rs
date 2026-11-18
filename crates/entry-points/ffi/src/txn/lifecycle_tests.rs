//! What a transaction handle sees before it commits, and what a read-only
//! one may do.

use crate::test_support::{add_collection, close, err, node, ok, query};
use crate::txn::{defra_transaction_commit, defra_transaction_create, defra_transaction_discard};

fn open(node: usize, read_only: bool) -> usize {
    let txn = defra_transaction_create(node, i32::from(read_only));
    assert_eq!(txn.status, 0, "defra_transaction_create must succeed");
    assert_ne!(txn.txn_ptr, 0);
    txn.txn_ptr
}

#[test]
fn a_transaction_sees_the_commits_of_its_own_uncommitted_writes() {
    let node = node();
    add_collection(node, "type TxnTest { value: Int }");
    let txn = open(node, false);

    let added = query(
        txn,
        r#"mutation { add_TxnTest(input: {value: 42}) { _docID } }"#,
    );
    let doc_id = added["data"]["add_TxnTest"][0]["_docID"]
        .as_str()
        .unwrap_or_else(|| panic!("no document ID in {added}"))
        .to_string();
    let commits = format!(r#"{{ _commits(docID: "{doc_id}") {{ cid }} }}"#);

    let inside = query(txn, &commits);
    assert!(
        inside["data"]["_commits"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "{inside}"
    );
    let outside = query(node, &commits);
    assert_eq!(
        outside["data"]["_commits"],
        serde_json::json!([]),
        "{outside}"
    );

    ok(defra_transaction_commit(txn));
    let committed = query(node, &commits);
    assert!(
        committed["data"]["_commits"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "{committed}"
    );
    close(node);
}

#[test]
fn a_read_only_transaction_reads_and_commits() {
    let node = node();
    add_collection(node, "type ReadOnlyTest { value: Int }");
    query(
        node,
        r#"mutation { add_ReadOnlyTest(input: {value: 7}) { _docID } }"#,
    );
    let txn = open(node, true);

    let read = query(txn, "{ ReadOnlyTest { value } }");
    assert_eq!(read["data"]["ReadOnlyTest"][0]["value"], 7, "{read}");

    ok(defra_transaction_commit(txn));
    close(node);
}

#[test]
fn a_read_only_transaction_refuses_a_write() {
    let node = node();
    add_collection(node, "type ReadOnlyWrite { value: Int }");
    let txn = open(node, true);

    let response = query(
        txn,
        r#"mutation { add_ReadOnlyWrite(input: {value: 1}) { _docID } }"#,
    );
    assert!(response.get("errors").is_some(), "{response}");

    defra_transaction_discard(txn);
    assert_eq!(
        query(node, "{ ReadOnlyWrite { value } }")["data"]["ReadOnlyWrite"],
        serde_json::json!([])
    );
    close(node);
}

#[test]
fn a_transaction_commits_once() {
    let node = node();
    let txn = open(node, false);
    ok(defra_transaction_commit(txn));
    err(defra_transaction_commit(txn));
    // Discarding a finished transaction does nothing.
    defra_transaction_discard(txn);
    close(node);
}
