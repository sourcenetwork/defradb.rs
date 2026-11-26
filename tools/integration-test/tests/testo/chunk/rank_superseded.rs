//! A chunk an edit replaced stays stored and indexed, but never ranks its
//! article, routed or not (`defra.chunk.rank.superseded`).

use integration_test::actions::{AddDoc, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::embed_support::{add_embedded_collection, model_embedded};
use super::rank_support::{add_article_about, body_about, ranked, score};

#[test]
#[mantra_macros::req_test("defra.chunk.rank.superseded")]
fn a_replaced_chunk_does_not_rank_its_article() {
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        AddDoc {
            collection: "Article".into(),
            doc_map: Some(json!({ "title": "edited", "body": body_about(&["rivers"]) })),
            common: Common {
                capture: captures([("edited", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        // The rivers chunk stays stored and indexed, nearest of all, but unlisted.
        UpdateDoc {
            collection: "Article".into(),
            doc_id: Text::reference("edited"),
            doc_map: json!({ "body": body_about(&["music"]) }),
            ..Default::default()
        }
        .into(),
        add_article_about("half", &["rivers and engines"], None),
        ranked(
            ", limit: 1",
            json!([{ "title": "half", "sim": score([1.0, 1.0, 0.0]) }]),
        )
        .into(),
        ranked(
            ", limit: 2",
            json!([
                { "title": "half", "sim": score([1.0, 1.0, 0.0]) },
                { "title": "edited", "sim": score([0.0, 0.0, 1.0]) },
            ]),
        )
        .into(),
        // Without a limit nothing is routed, and every article scores from its listed chunks.
        ranked(
            "",
            json!([
                { "title": "half", "sim": score([1.0, 1.0, 0.0]) },
                { "title": "edited", "sim": score([0.0, 0.0, 1.0]) },
            ]),
        )
        .into(),
    ])
    .execute();
}
