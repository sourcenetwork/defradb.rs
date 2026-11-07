//! A filtered similarity query on articles widens its chunk search until the
//! page fills (`defra.chunk.rank.widen`).

use integration_test::test_case::TestCase;
use serde_json::json;

use super::embed_support::{add_embedded_collection, model_embedded};
use integration_test::actions::Request;
use integration_test::scenario::Text;

use super::rank_support::{
    add_article, add_article_about, body_about, explained, ranked, score, CHUNK_VECTOR_INDEX,
};

const KEPT: &str = r#", limit: 1, filter: {title: {_eq: "kept"}}"#;

#[test]
#[mantra_macros::req_test("defra.chunk.rank.widen")]
fn a_filtered_query_widens_past_the_chunks_of_filtered_out_articles() {
    let mut actions = vec![model_embedded(&[]), add_embedded_collection(None)];
    for draft in ["draft 1", "draft 2", "draft 3"] {
        actions.push(add_article_about(draft, &["rivers"], None));
    }
    actions.push(add_article_about("kept", &["rivers and engines"], None));
    actions.push(
        ranked(
            KEPT,
            json!([{ "title": "kept", "sim": score([1.0, 1.0, 0.0]) }]),
        )
        .into(),
    );
    // One chunk, then two, then four: the fourth nearest is the kept article's.
    // Nine reads: four chunks, their four articles, and the kept one's listed chunk.
    actions.push(
        explained(
            KEPT,
            json!({
                "iterations": 2,
                "docFetches": 9,
                "fieldFetches": 32,
                "indexFetches": 3,
                "vectorIndex": CHUNK_VECTOR_INDEX,
            }),
        )
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.rank.doc_ids")]
fn requested_doc_ids_narrow_a_routed_query() {
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        add_article_about("near", &["rivers"], None),
        add_article(
            json!({ "title": "asked", "body": body_about(&["rivers and engines"]) }),
            None,
            Some("asked"),
        ),
        Request {
            request: Text::concat([
                Text::from(r#"query { Article(order: {_alias: {sim: DESC}}, limit: 1, docID: [""#),
                Text::reference("asked"),
                Text::from(r#""]) { title sim: SIMILARITY(body_v: {vector: [1, 0, 0]}) } }"#),
            ]),
            expected_data: json!({ "Article": [{ "title": "asked", "sim": score([1.0, 1.0, 0.0]) }] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
