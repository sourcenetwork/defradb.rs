//! An explicit transaction that loses a write conflict at commit reports a
//! typed, retryable conflict (#1994).

use defra_node::{EmbeddedNode, QueryRequest};
use query::TransactionError;

#[test]
fn losing_commit_reports_a_typed_conflict() {
    workers::block_on(async {
    let node = EmbeddedNode::builder().build().await.unwrap();
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

    let first = node.begin_transaction(false).await.unwrap();
    let second = node.begin_transaction(false).await.unwrap();
    for (handle, name) in [(&first, "Bob"), (&second, "Carol")] {
        let response = node
            .execute_request_in_txn(
                QueryRequest::new(format!(
                    r#"mutation {{ update_Users(docID: "{doc_id}", input: {{name: "{name}"}}) {{ name }} }}"#
                )),
                handle,
            )
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
    }

    node.commit_transaction(&first).await.unwrap();
    let error = node.commit_transaction(&second).await.unwrap_err();
    assert!(matches!(error, TransactionError::Conflict(_)), "{error:?}");
    assert!(error.is_retryable());
    assert!(error.to_string().contains("transaction conflict"));
})
    .unwrap()
}
