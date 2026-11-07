//! A similarity query on articles ranks each by its best chunk, through the
//! chunk vector index (`defra.chunk.rank.best_chunk`).

use integration_test::test_case::TestCase;
use serde_json::json;

use super::embed_support::{add_embedded_collection, model_embedded};
use super::rank_support::{add_article_about, explained, ranked, score, CHUNK_VECTOR_INDEX};

#[test]
#[mantra_macros::req_test("defra.chunk.rank.best_chunk")]
fn articles_rank_by_their_best_chunk_through_the_chunk_index() {
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        add_article_about("late", &["music", "rivers"], None),
        add_article_about("half", &["rivers and engines"], None),
        add_article_about("music", &["music", "music"], None),
        ranked(
            ", limit: 2",
            json!([
                { "title": "late", "sim": score([1.0, 0.0, 0.0]) },
                { "title": "half", "sim": score([1.0, 1.0, 0.0]) },
            ]),
        )
        .into(),
        // Seven reads: the two nearest chunks, their two articles, and the three
        // listed chunks that score them exactly.
        explained(
            ", limit: 2",
            json!({
                "iterations": 3,
                "docFetches": 7,
                "fieldFetches": 26,
                "indexFetches": 1,
                "vectorIndex": CHUNK_VECTOR_INDEX,
            }),
        )
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.rank.offset")]
fn an_offset_skips_the_best_articles() {
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        add_article_about("late", &["music", "rivers"], None),
        add_article_about("half", &["rivers and engines"], None),
        add_article_about("music", &["music", "music"], None),
        ranked(
            ", limit: 1, offset: 1",
            json!([{ "title": "half", "sim": score([1.0, 1.0, 0.0]) }]),
        )
        .into(),
    ])
    .execute();
}
