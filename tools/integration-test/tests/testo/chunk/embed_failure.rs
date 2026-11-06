//! A model that fails fails the write, and nothing of it is stored
//! (`defra.chunk.embed.failure`).

use integration_test::actions::{AddDoc, EmbeddingModel, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::embed_support::{add_embedded_collection, model_embedded};
use super::support::{body, no_chunks};

pub const MODEL_FAILED: &str =
    "embedding error: embedding provider returned 500 Internal Server Error";

pub fn model_fails() -> EmbeddingModel {
    EmbeddingModel {
        failing: Some(true),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.failure")]
fn a_failing_model_fails_the_write_and_stores_nothing() {
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        model_fails().into(),
        AddDoc {
            collection: "Article".into(),
            doc_map: Some(json!({ "title": "t", "body": body(3, "rivers") })),
            expected_error: Some(MODEL_FAILED.into()),
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
