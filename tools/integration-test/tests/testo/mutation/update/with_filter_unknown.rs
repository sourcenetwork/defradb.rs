//! `update_` checks its filter, and the filters of its selection, against
//! their input types. Texts measured on Go DefraDB 307364d9d.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

fn rejects(id: &'static str, request: &str, error: &str) {
    TestCase {
        id,
        actions: vec![
            AddCollection {
                sdl: "type Author { name: String books: [Book] } type Book { title: String author: Author }"
                    .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "A" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: request.into(),
                expected_error: Some(error.into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Author { name } }".into(),
                expected_data: json!({ "Author": [{ "name": "A" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn update_with_an_unknown_filter_field_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        r#"mutation { update_Author(filter: {nope: {_eq: 1}}, input: {name: "B"}) { name } }"#,
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field.",
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn update_selection_with_an_unknown_filter_field_is_rejected() {
    rejects(
        integration_test::scenario_id!(),
        r#"mutation { update_Author(input: {name: "B"}) { name books(filter: {nope: {_eq: 1}}) { title } } }"#,
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field.",
    );
}
