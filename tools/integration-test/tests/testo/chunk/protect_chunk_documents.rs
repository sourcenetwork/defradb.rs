//! A chunk document cannot be updated, deleted or truncated, and the chunk settings cannot be patched (`defra.chunk.protect.chunk_documents`).

use integration_test::actions::{DeleteDoc, PatchCollection, Request, Truncate, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{add_article, add_article_collection, body, CHUNKS};

#[test]
#[mantra_macros::req_test("defra.chunk.protect.chunk_documents")]
fn chunk_documents_and_settings_are_read_only() {
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&body(3, "rivers"), "article"),
        Request {
            request: format!("query {{ {CHUNKS}(order: {{hash: ASC}}) {{ _docID hash text }} }}")
                .into(),
            common: Common {
                capture: captures([
                    ("chunk", "/data/Article__body_chunk/0/_docID"),
                    ("before", "/data/Article__body_chunk"),
                ]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        UpdateDoc {
            collection: CHUNKS.into(),
            doc_id: Text::reference("chunk"),
            doc_map: json!({ "text": "forged" }),
            expected_error: Some("Article__body_chunk holds the chunks of Article.body and is written only by writes to Article".into()),
            ..Default::default()
        }
        .into(),
        DeleteDoc {
            collection: CHUNKS.into(),
            doc_id: Text::reference("chunk"),
            expected_error: Some("Article__body_chunk holds the chunks of Article.body and is written only by writes to Article".into()),
            ..Default::default()
        }
        .into(),
        Truncate {
            collection: CHUNKS.into(),
            expected_error: Some("Article__body_chunk holds generated chunks and is emptied only with its parent".into()),
            ..Default::default()
        }
        .into(),
        PatchCollection {
            patch: r#"[{"op":"replace","path":"/Article/ChunkedFields/0/MaxChars","value":100}]"#
                .into(),
            expected_error: Some("invalid patch: collection chunk settings cannot be mutated. Collection: Article".into()),
            ..Default::default()
        }
        .into(),
        Request {
            request: format!(
                r#"mutation {{ add_{CHUNKS}(input: {{text: "forged"}}) {{ _docID }} }}"#
            )
            .into(),
            expected_error: Some("Article__body_chunk holds the chunks of Article.body and is written only by writes to Article".into()),
            ..Default::default()
        }
        .into(),
        Request {
            request: Text::concat([
                Text::from(r#"mutation { update_Article(docID: ""#),
                Text::reference("article"),
                Text::from(r#"", input: {_body_chunks: ["forged"]}) { _docID } }"#),
            ]),
            expected_error: Some("Article._body_chunks is read-only: it is set from body by every write that changes it".into()),
            ..Default::default()
        }
        .into(),
        Request {
            request: format!("query {{ {CHUNKS}(order: {{hash: ASC}}) {{ _docID hash text }} }}")
                .into(),
            expected_data: json!({ CHUNKS: m::r("before") }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
