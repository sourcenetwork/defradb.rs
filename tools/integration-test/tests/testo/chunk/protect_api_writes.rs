//! The collection API cannot write chunks or the chunk list (`defra.chunk.protect.api_writes`).

use integration_test::actions::{AddDoc, Request, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{add_article_collection, CHUNKS};

#[test]
#[mantra_macros::req_test("defra.chunk.protect.api_writes")]
fn the_collection_api_cannot_write_chunks_or_the_chunk_list() {
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
        AddDoc {
            collection: "Article".into(),
            doc_map: Some(json!({ "title": "t", "body": "x", "_body_chunks": ["aa", "bb"] })),
            expected_error: Some("Article._body_chunks is read-only: it is set from body by every write that changes it".into()),
            ..Default::default()
        }
        .into(),
        UpdateDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            doc_map: json!({ "_body_chunks": ["aa", "bb"] }),
            expected_error: Some("Article._body_chunks is read-only: it is set from body by every write that changes it".into()),
            ..Default::default()
        }
        .into(),
        Request {
            request: format!("query {{ {CHUNKS} {{ text }} Article {{ _body_chunks }} }}").into(),
            expected_data: json!({ CHUNKS: [], "Article": [{ "_body_chunks": null }] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
