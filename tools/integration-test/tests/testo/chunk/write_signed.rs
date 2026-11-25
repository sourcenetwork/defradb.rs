//! A node that signs its blocks leaves the chunks unsigned and keeps the parent's chunks exact (`defra.chunk.write.signed`).

use integration_test::actions::{Actions, NewNode, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{
    add_article, add_article_collection, body, chunk_texts_are, gql, update_article, CHUNKS,
};

const CREATED: usize = 6;
const UPDATED: usize = 4;

fn signing_node() -> Actions {
    NewNode {
        enable_signing: true,
        ..Default::default()
    }
    .into()
}

/// The text block of the article named `capture` carries a signature.
fn text_is_signed(capture: &str) -> Actions {
    Request {
        request: Text::concat([
            Text::from(r#"query { _commits(docID: ""#),
            Text::reference(capture),
            Text::from(r#"", filter: {fieldName: {_eq: "body"}}) { signature { type } } }"#),
        ]),
        expected_data: json!({ "_commits": [{ "signature": { "type": m::nonempty_string() } }] }),
        ..Default::default()
    }
    .into()
}

/// The text block of the first chunk carries no signature, so its ID is the same on every node.
fn a_chunk_is_unsigned() -> Vec<Actions> {
    vec![
        Request {
            request: format!("query {{ {CHUNKS} {{ _docID }} }}").into(),
            common: Common {
                capture: captures([("chunk", &format!("/data/{CHUNKS}/0/_docID"))]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        Request {
            request: Text::concat([
                Text::from(r#"query { _commits(docID: ""#),
                Text::reference("chunk"),
                Text::from(r#"", filter: {fieldName: {_eq: "text"}}) { signature { type } } }"#),
            ]),
            expected_data: json!({ "_commits": [{ "signature": null }] }),
            ..Default::default()
        }
        .into(),
    ]
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.signed")]
fn a_signing_node_keeps_the_chunks_of_the_first_update() {
    TestCase::from_actions(
        vec![
            signing_node(),
            add_article_collection(),
            add_article(&body(CREATED, "rivers"), "article"),
            text_is_signed("article"),
            update_article("article", &body(UPDATED, "mountains")),
            chunk_texts_are(&[&body(UPDATED, "mountains")], None),
        ]
        .into_iter()
        .chain(a_chunk_is_unsigned())
        .collect(),
    )
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.signed")]
fn a_signing_node_keeps_the_chunks_of_an_array_create() {
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
        chunk_texts_are(
            &[&body(UPDATED, "mountains"), &body(UPDATED, "valleys")],
            None,
        ),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.signed")]
fn a_signing_node_keeps_the_chunks_of_an_upsert_create() {
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
        chunk_texts_are(&[&body(UPDATED, "mountains")], None),
    ])
    .execute();
}
