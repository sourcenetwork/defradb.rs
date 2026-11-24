//! A filtered similarity query on articles scores every matching article from
//! its chunks, as a single-vector field does, and says why
//! (`defra.chunk.rank.filtered`).

use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

use super::embed_support::{add_embedded_collection, model_embedded};
use integration_test::actions::Request;
use integration_test::scenario::Text;

use super::rank_support::{add_article, add_article_about, body_about, ranked, score};

const KEPT: &str = r#", limit: 1, filter: {title: {_eq: "kept"}}"#;

#[test]
#[mantra_macros::req_test("defra.chunk.rank.filtered")]
fn a_filtered_query_scores_the_matching_articles_without_the_index() {
    let mut actions = vec![model_embedded(&[]), add_embedded_collection(None)];
    for draft in ["draft 1", "draft 2", "draft 3"] {
        actions.push(add_article_about(draft, &["rivers"], None));
    }
    actions.push(add_article_about("kept", &["rivers and engines"], None));
    let mut kept = ranked(
        KEPT,
        json!([{ "title": "kept", "sim": score([1.0, 1.0, 0.0]) }]),
    );
    kept.expected_extensions = json!(m::object(json!({
        "warnings": [m::object(json!({
            "code": "VECTOR_INDEX_UNUSED",
            "detail": { "field": "body_v", "reason": "filter" },
            "message": m::nonempty_string(),
        }))],
    })));
    actions.push(kept.into());
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
