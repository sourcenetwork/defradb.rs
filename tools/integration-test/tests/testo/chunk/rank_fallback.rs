//! A routed query whose chunk index can't fill the page scores every article
//! from its listed chunks (`defra.chunk.rank.fallback`).

use integration_test::test_case::TestCase;
use serde_json::{json, Value};

use super::embed_support::{add_embedded_collection, model_embedded};
use super::rank_support::{
    add_article, add_article_about, explained, ranked, score, CHUNK_VECTOR_INDEX,
};

#[test]
#[mantra_macros::req_test("defra.chunk.rank.fallback")]
fn an_index_that_cannot_fill_the_page_falls_back_to_every_article() {
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        add_article_about("late", &["music", "rivers"], None),
        add_article_about("half", &["rivers and engines"], None),
        // No body, so no chunks and nothing in the index.
        add_article(json!({ "title": "empty" }), None, None),
        ranked(
            ", limit: 3",
            json!([
                { "title": "late", "sim": score([1.0, 0.0, 0.0]) },
                { "title": "half", "sim": score([1.0, 1.0, 0.0]) },
                { "title": "empty", "sim": Value::Null },
            ]),
        )
        .into(),
        // Three chunks name two articles; asking for six finds nothing new, so all
        // three articles are read and scored. Eleven reads: three chunks and two
        // articles from the index, then three articles and their three listed chunks.
        explained(
            ", limit: 3",
            json!({
                "iterations": 4,
                "docFetches": 11,
                "fieldFetches": 37,
                "indexFetches": 2,
                "vectorIndex": CHUNK_VECTOR_INDEX,
            }),
        )
        .into(),
    ])
    .execute();
}
