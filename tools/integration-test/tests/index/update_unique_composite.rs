//! Generated from the conformance scenarios of tests/integration/index/update_unique_composite_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.unique.write-conflict")]
fn unique_composite_index_update_upon_updating_doc_with_existing_field_value_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(unique: true, includes: [{field: "name"}, {field: "age"}]) {
                        name: String 
                        age: Int 
                        email: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21, "email": "email@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "email": "another@gmail.com" }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
