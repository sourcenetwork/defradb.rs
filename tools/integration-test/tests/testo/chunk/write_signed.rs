//! A node that signs its blocks keeps the parent's chunks exact (`defra.chunk.write.signed`).

use integration_test::actions::{Actions, NewNode, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{
    add_article, add_article_collection, body, chunks_are_listed, gql, read_list, update_article,
};

const CREATED: usize = 6;
// Fewer chunks than the create: a shorter list sorts first, so it loses a tie
// with the create's list and shows a first update stored at the create's priority.
const UPDATED: usize = 4;

fn signing_node() -> Actions {
    NewNode {
        enable_signing: true,
        ..Default::default()
    }
    .into()
}

/// The list block of the article named `capture` carries a signature.
fn list_is_signed(capture: &str) -> Actions {
    Request {
        request: Text::concat([
            Text::from(r#"query { _commits(docID: ""#),
            Text::reference(capture),
            Text::from(
                r#"", filter: {fieldName: {_eq: "_body_chunks"}}) { signature { type } } }"#,
            ),
        ]),
        expected_data: json!({ "_commits": [{ "signature": { "type": m::nonempty_string() } }] }),
        ..Default::default()
    }
    .into()
}

/// The list of the article named `capture` has the text's hash and `n` chunk hashes.
fn list_has(capture: &str, n: usize) -> Actions {
    Request {
        request: Text::concat([
            Text::from(r#"query { Article(docID: ""#),
            Text::reference(capture),
            Text::from(r#"") { _body_chunks } }"#),
        ]),
        expected_data: json!({ "Article": [{ "_body_chunks": m::length(n as u64 + 1) }] }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.signed")]
fn a_signing_node_stores_the_list_of_the_first_update() {
    TestCase::from_actions(vec![
        signing_node(),
        add_article_collection(),
        add_article(&body(CREATED, "rivers"), "article"),
        list_is_signed("article"),
        update_article("article", &body(UPDATED, "mountains")),
        read_list("updated", UPDATED, None),
        chunks_are_listed("updated", UPDATED, None),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.signed")]
fn a_signing_node_stores_the_lists_of_an_array_create() {
    TestCase::from_actions(vec![
        signing_node(),
        add_article_collection(),
        Request {
            request: format!(
                r#"mutation {{ add_Article(input: [{{title: "a", body: "{}"}}, {{title: "b", body: "{}"}}]) {{ _docID }} }}"#,
                gql(&body(CREATED, "rivers")),
                gql(&body(CREATED, "lakes"))
            )
            .into(),
            common: Common {
                capture: captures([
                    ("first", "/data/add_Article/0/_docID"),
                    ("second", "/data/add_Article/1/_docID"),
                ]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        update_article("first", &body(UPDATED, "mountains")),
        update_article("second", &body(UPDATED, "valleys")),
        list_has("first", UPDATED),
        list_has("second", UPDATED),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.signed")]
fn a_signing_node_stores_the_list_of_an_upsert_create() {
    TestCase::from_actions(vec![
        signing_node(),
        add_article_collection(),
        Request {
            request: format!(
                r#"mutation {{ upsert_Article(filter: {{title: {{_eq: "u"}}}}, add: {{title: "u", body: "{}"}}, update: {{title: "u"}}) {{ _docID }} }}"#,
                gql(&body(CREATED, "rivers"))
            )
            .into(),
            common: Common {
                capture: captures([("article", "/data/upsert_Article/0/_docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        update_article("article", &body(UPDATED, "mountains")),
        list_has("article", UPDATED),
    ])
    .execute();
}
