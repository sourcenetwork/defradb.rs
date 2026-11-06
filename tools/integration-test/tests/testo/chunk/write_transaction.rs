//! An explicit transaction writes chunks only when it commits (`defra.chunk.write.transaction`).

use integration_test::actions::{Request, TxnBegin, TxnCommit};
use integration_test::test_case::TestCase;

use super::support::{add_article_collection, body, chunks_are_listed, gql, no_chunks, read_list};

#[test]
#[mantra_macros::req_test("defra.chunk.write.transaction")]
fn an_explicit_transaction_writes_chunks_on_commit() {
    TestCase::from_actions(vec![
        add_article_collection(),
        TxnBegin::default().into(),
        Request {
            request: format!(
                r#"mutation {{ add_Article(input: {{title: "t", body: "{}"}}) {{ _docID }} }}"#,
                gql(&body(5, "forests"))
            )
            .into(),
            txn: Some(0),
            ..Default::default()
        }
        .into(),
        no_chunks(None),
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
        read_list("committed", 5, None),
        chunks_are_listed("committed", 5, None),
    ])
    .execute();
}
