//! A chunk index built with another metric ranks and scores by it
//! (`defra.chunk.rank.metric`).

use integration_test::actions::AddCollection;
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

use super::embed_support::{article_schema, model_embedded};
use super::rank_support::{add_article_about, ranked, RIVERS};

#[test]
#[mantra_macros::req_test("defra.chunk.rank.metric")]
fn a_dot_product_index_ranks_by_dot_product() {
    let dot = |counts: [f64; 3]| m::similarity_score("DOT", json!(counts), json!(RIVERS));
    TestCase::from_actions(vec![
        model_embedded(&[]),
        AddCollection {
            sdl: article_schema(None, "dimensions: 3, hnsw: {metric: DOT}", "", ""),
            ..Default::default()
        }
        .into(),
        // Under cosine both would score 1; the dot product counts the repeat.
        add_article_about("once", &["rivers"], None),
        add_article_about("twice", &["rivers and rivers"], None),
        ranked(
            ", limit: 2",
            json!([
                { "title": "twice", "sim": dot([2.0, 0.0, 0.0]) },
                { "title": "once", "sim": dot([1.0, 0.0, 0.0]) },
            ]),
        )
        .into(),
    ])
    .execute();
}
