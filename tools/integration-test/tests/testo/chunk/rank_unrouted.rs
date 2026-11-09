//! A similarity query on articles that the chunk index cannot answer scores
//! every article from all its chunks, and says why (`defra.chunk.rank.unrouted`).

use integration_test::actions::Request;
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

use super::embed_support::{add_embedded_collection, model_embedded};
use super::rank_support::{add_article_about, ranked, score, CHUNK_FIELDS};

#[test]
#[mantra_macros::req_test("defra.chunk.rank.unrouted")]
fn a_query_without_a_limit_scores_every_article_from_all_its_chunks() {
    let mut everything = ranked(
        "",
        json!([
            { "title": "late", "sim": score([1.0, 0.0, 0.0]) },
            { "title": "half", "sim": score([1.0, 1.0, 0.0]) },
            { "title": "music", "sim": score([0.0, 0.0, 1.0]) },
        ]),
    );
    everything.expected_extensions = json!(m::object(json!({
        "warnings": [m::object(json!({
            "code": "VECTOR_INDEX_UNUSED",
            "detail": { "field": "body_v", "reason": "noLimit" },
            "message": m::nonempty_string(),
        }))],
    })));
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        add_article_about("late", &["music", "rivers"], None),
        add_article_about("half", &["rivers and engines"], None),
        add_article_about("music", &["music"], None),
        everything.into(),
        // The articles come from the scan; scoring reads each one's listed chunks
        // with one index lookup each, and never the article again.
        Request {
            request: r#"query @explain(type: execute) { Article(order: {_alias: {sim: DESC}}) {
                title sim: SIMILARITY(body_v: {vector: [1, 0, 0]})
            } }"#
                .into(),
            expected_data: m::explain_target(
                "similarityNode",
                false,
                0,
                json!({
                    "iterations": 4,
                    "docFetches": 4,
                    "fieldFetches": 4 * CHUNK_FIELDS,
                    "indexFetches": 3,
                }),
            ),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.rank.wrong_length")]
fn a_vector_of_the_wrong_length_names_the_chunk_field() {
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        add_article_about("rivers", &["rivers"], None),
        Request {
            request: r#"query { Article(order: {_alias: {sim: DESC}}, limit: 1) {
                title
                sim: SIMILARITY(body_v: {vector: [1, 0]})
            } }"#
                .into(),
            expected_error: Some(WRONG_LENGTH.into()),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

const WRONG_LENGTH: &str = "source and vector must be of the same length. Source: 3, Vector: 2, Field: Article__body_chunk.body_v";
