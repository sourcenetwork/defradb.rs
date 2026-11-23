//! An explicit transaction writes, replaces and deletes chunks only when it commits (`defra.chunk.write.transaction`).

use integration_test::actions::{
    Actions, DeleteDoc, Request, TxnBegin, TxnCommit, TxnDiscard, UpdateDoc,
};
use integration_test::scenario::Text;
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{
    add_article, add_article_collection, body, chunk_texts_are, chunks_are_listed, gql, in_txn,
    list_is, no_chunks, read_list,
};

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

fn update_in(txn: usize, text: &str) -> Actions {
    UpdateDoc {
        collection: "Article".into(),
        doc_id: Text::reference("article"),
        doc_map: json!({ "body": text }),
        txn: Some(txn),
        ..Default::default()
    }
    .into()
}

fn delete_in(txn: usize) -> Actions {
    DeleteDoc {
        collection: "Article".into(),
        doc_id: Text::reference("article"),
        txn: Some(txn),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.transaction")]
fn an_explicit_transaction_replaces_and_deletes_chunks_on_commit() {
    let (rivers, lakes) = (body(4, "rivers"), body(2, "lakes"));
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&rivers, "article"),
        TxnBegin::default().into(),
        update_in(0, &lakes),
        in_txn(chunk_texts_are(&[&lakes], None), 0),
        chunk_texts_are(&[&rivers], None),
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
        chunk_texts_are(&[&lakes], None),
        TxnBegin::default().into(),
        delete_in(1),
        in_txn(no_chunks(None), 1),
        chunk_texts_are(&[&lakes], None),
        TxnCommit {
            txn: 1,
            ..Default::default()
        }
        .into(),
        no_chunks(None),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.transaction")]
fn a_discarded_transaction_leaves_the_chunks_as_they_were() {
    let rivers = body(4, "rivers");
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&rivers, "article"),
        read_list("created", 4, None),
        TxnBegin::default().into(),
        update_in(0, &body(2, "lakes")),
        TxnDiscard {
            txn: 0,
            ..Default::default()
        }
        .into(),
        TxnBegin::default().into(),
        delete_in(1),
        TxnDiscard {
            txn: 1,
            ..Default::default()
        }
        .into(),
        list_is("created", 4, None),
        chunk_texts_are(&[&rivers], None),
    ])
    .execute();
}
