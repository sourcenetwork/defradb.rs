//! An explicit transaction that loses a write conflict at commit reports a
//! typed, retryable conflict (#1994).

use defra_node::{Node, QueryRequest};
use query::TransactionError;

#[test]
fn losing_commit_reports_a_typed_conflict() {
    workers::block_on(async {
    let node = Node::builder().build().await.unwrap();
    node.add_collection("type Users { name: String }")
        .await
        .unwrap();
    let created = node
        .execute(r#"mutation { add_Users(input: {name: "Alice"}) { _docID } }"#)
        .await;
    let doc_id = created.data.unwrap()["add_Users"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string();

    let first = node.new_txn(false).await.map(|txn| txn.detach()).unwrap();
    let second = node.new_txn(false).await.map(|txn| txn.detach()).unwrap();
    for (handle, name) in [(&first, "Bob"), (&second, "Carol")] {
        let response = node
            .with_transaction(*handle).execute_request(QueryRequest::new(format!(
                    r#"mutation {{ update_Users(docID: "{doc_id}", input: {{name: "{name}"}}) {{ name }} }}"#
                )))
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
    }

    node.with_transaction(first).commit().await.unwrap();
    let error = node.with_transaction(second).commit().await.unwrap_err();
    assert!(
        matches!(
            error,
            defra_node::Error::TransactionConflict(TransactionError::Conflict(_))
        ),
        "{error:?}"
    );
    assert!(error.is_retryable());
    assert!(error.to_string().contains("transaction conflict"));
})
    .unwrap()
}
