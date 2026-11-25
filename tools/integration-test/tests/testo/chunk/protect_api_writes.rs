//! The collection API cannot write chunks (`defra.chunk.protect.api_writes`).

use integration_test::actions::{AddDoc, Request};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{add_article_collection, CHUNKS};

#[test]
#[mantra_macros::req_test("defra.chunk.protect.api_writes")]
fn the_collection_api_cannot_write_chunks() {
    TestCase::from_actions(vec![
        add_article_collection(),
        AddDoc {
            collection: "Article".into(),
            doc_map: Some(json!({ "title": "t", "body": "Some text." })),
            common: Common {
                capture: captures([("article", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: CHUNKS.into(),
            doc_map: Some(json!({ "text": "forged", "hash": "00" })),
            expected_error: Some("Article__body_chunk holds the chunks of Article.body and is written only by writes to Article".into()),
            ..Default::default()
        }
        .into(),
        Request {
            request: format!("query {{ {CHUNKS} {{ text }} }}").into(),
            expected_data: json!({
                CHUNKS: [{ "text": "Some text." }],
            }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
