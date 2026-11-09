//! A commit's links and heads show the delta of the block they point to, as
//! Go's do (#2625, `defra.commits.link_delta`).

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.commits.link_delta")]
fn a_composite_link_shows_its_field_delta() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Book { title: String! genre: String plot: String }".into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Book".into(),
            doc_map: Some(json!({ "title": "1984", "plot": "Big Brother is watching" })),
            common: Common {
                capture: captures([("book", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        UpdateDoc {
            collection: "Book".into(),
            doc_id: Text::reference("book"),
            doc_map: json!({ "genre": "Dystopia" }),
            expected_data: json!({}),
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"query {
                _commits(filter: {fieldName: {_eq: "_C"}}, depth: 1) {
                    fieldName
                    height
                    links { fieldName delta }
                    heads { fieldName delta }
                }
            }"#
            .into(),
            expected_data: json!({ "_commits": [{
                "fieldName": "_C",
                "height": 2,
                "links": [{ "fieldName": "genre", "delta": "aER5c3RvcGlh" }],
                "heads": [{ "fieldName": "_C", "delta": null }],
            }] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
