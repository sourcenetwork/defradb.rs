//! An encrypted text gives encrypted chunks, and an update keeps them encrypted (`defra.chunk.write.encrypted`).

use integration_test::actions::{Actions, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{add_article_collection, body, CHUNKS};

/// The one chunk holds `text`, readable in the clear; its ID is named `capture`.
fn one_chunk(text: &str, capture: &str) -> Actions {
    Request {
        request: format!("query {{ {CHUNKS} {{ _docID text }} }}").into(),
        expected_data: json!({ CHUNKS: [{ "_docID": m::doc_id(), "text": text }] }),
        common: Common {
            capture: captures([(capture, &format!("/data/{CHUNKS}/0/_docID"))]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// The stored text of the chunk named `capture` is not `text` in the clear.
fn stored_encrypted(capture: &str, text: &str) -> Actions {
    Request {
        request: Text::concat([
            Text::from(r#"query { _commits(docID: ""#),
            Text::reference(capture),
            Text::from(r#"", filter: {fieldName: {_eq: "text"}}) { delta } }"#),
        ]),
        expected_data: json!({ "_commits": [{ "delta": m::not_plain_cbor_string(json!(text)) }] }),
        ..Default::default()
    }
    .into()
}

fn encrypted_article(text: &str, whole_doc: bool) -> Actions {
    AddDoc {
        collection: "Article".into(),
        doc_map: Some(json!({ "title": "t", "body": text })),
        is_doc_encrypted: Some(whole_doc),
        encrypted_fields: Some(if whole_doc {
            vec![]
        } else {
            vec!["body".into()]
        }),
        common: Common {
            capture: captures([("article", "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn case(whole_doc: bool) {
    // One short paragraph is one chunk with the whole text.
    let (created, updated) = (body(1, "rivers"), body(1, "lakes"));
    TestCase::from_actions(vec![
        add_article_collection(),
        encrypted_article(&created, whole_doc),
        one_chunk(&created, "created"),
        stored_encrypted("created", &created),
        UpdateDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            doc_map: json!({ "body": updated }),
            ..Default::default()
        }
        .into(),
        one_chunk(&updated, "updated"),
        stored_encrypted("updated", &updated),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.encrypted")]
fn an_encrypted_field_gives_encrypted_chunks() {
    case(false);
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.encrypted")]
fn an_encrypted_document_gives_encrypted_chunks() {
    case(true);
}
