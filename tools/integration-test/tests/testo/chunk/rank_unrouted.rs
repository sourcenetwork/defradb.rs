//! A similarity query on articles that the chunk index cannot answer scores
//! every article from all its chunks, and says why (`defra.chunk.rank.unrouted`).

use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

use super::embed_support::{add_embedded_collection, model_embedded};
use super::rank_support::{add_article_about, ranked, score};

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
    ])
    .execute();
}
