//! A `@chunk` field generates a queryable chunk collection (`defra.chunk.declare.collection`).

use integration_test::actions::{IntrospectionRequest, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{add_article_collection, CHUNKS};

#[test]
#[mantra_macros::req_test("defra.chunk.declare.collection")]
fn chunk_generates_a_queryable_collection() {
    TestCase::from_actions(vec![
        add_article_collection(),
        IntrospectionRequest {
            request: "query { __schema { types { name } } }".into(),
            expected_data: json!({ "__schema": { "types": [{ "name": CHUNKS }] } }),
            contains_data: true,
            ..Default::default()
        }
        .into(),
        Request {
            request: format!("query {{ {CHUNKS} {{ _docID _sourceID text hash }} }}").into(),
            expected_data: json!({ CHUNKS: [] }),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Article { _body_chunks } }".into(),
            expected_data: json!({ "Article": [] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
