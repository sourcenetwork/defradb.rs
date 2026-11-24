//! A chunk embedding with no model or URL fails the write instead of storing
//! chunks without vectors (`defra.chunk.embed.unconfigured`).

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{body, no_chunks};

const NO_URL: &str = "a chunk embedding needs a URL: set it on the @embedding or in the node's embedding config. Field: Article__body_chunk.body_v";

#[test]
#[mantra_macros::req_test("defra.chunk.embed.unconfigured")]
fn a_chunk_embedding_without_a_url_fails_the_write() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: r#"type Article {
                title: String
                body: String @chunk(maxChars: 64)
                body_v: [Float32!] @embedding(fields: ["body"], provider: "openai", model: "m") @index(vector: {dimensions: 3})
            }"#
            .into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Article".into(),
            doc_map: Some(json!({ "title": "t", "body": body(3, "rivers") })),
            expected_error: Some(NO_URL.into()),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Article { _docID } }".into(),
            expected_data: json!({ "Article": [] }),
            ..Default::default()
        }
        .into(),
        no_chunks(None),
    ])
    .execute();
}
