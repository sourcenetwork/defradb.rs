//! Truncating the parent empties its chunks, and a filtered truncate only the matching parents' (`defra.chunk.purge.truncate`).

use integration_test::actions::shared::Filter;
use integration_test::actions::{AddDoc, Truncate};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{add_article, add_article_collection, body, chunk_texts_are, no_chunks};

#[test]
#[mantra_macros::req_test("defra.chunk.purge.truncate")]
fn truncating_the_parent_empties_its_chunks() {
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&body(4, "caves"), "article"),
        chunk_texts_are(&[&body(4, "caves")], None),
        Truncate {
            collection: "Article".into(),
            ..Default::default()
        }
        .into(),
        no_chunks(None),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.purge.truncate")]
fn a_filtered_truncate_empties_only_the_matching_parents_chunks() {
    let (caves, lakes) = (body(3, "caves"), body(2, "lakes"));
    TestCase::from_actions(vec![
        add_article_collection(),
        AddDoc {
            collection: "Article".into(),
            docs: Some(vec![
                json!({ "title": "gone", "body": caves }),
                json!({ "title": "kept", "body": lakes }),
            ]),
            ..Default::default()
        }
        .into(),
        Truncate {
            collection: "Article".into(),
            filter: Some(Filter {
                kind: "string".into(),
                value: Some(json!(r#"{title: {_eq: "gone"}}"#)),
            }),
            ..Default::default()
        }
        .into(),
        chunk_texts_are(&[&lakes], None),
    ])
    .execute();
}
