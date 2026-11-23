//! `delete_` checks its filter against its input type. Text measured on Go
//! DefraDB 307364d9d.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn delete_with_an_unknown_filter_field_is_rejected() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Author { name: String }".into(),
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
                request: "mutation { delete_Author(filter: {nope: {_eq: 1}}) { name } }".into(),
                expected_error: Some(
                    "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field."
                        .into(),
                ),
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
