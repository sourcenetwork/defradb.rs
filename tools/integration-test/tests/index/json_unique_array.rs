//! Generated from the conformance scenarios of tests/integration/index/json_unique_array_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.unique.json")]
fn json_array_unique_index_should_allow_only_unique_values_and_use_them_for_fetching() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        custom: JSON @index(unique: true)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [3, 4, null] }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "custom": { "numbers": ["3", "str", true] }, "name": "Bruno" }),
                ),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": 3 }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [4] }, "name": "Islam" })),
                expected_error: Some(
                    "can not index a doc's field(s) that violates unique index.".into(),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [5, 8, 5] }, "name": "Shahzad" })),
                expected_error: Some(
                    "can not index a doc's field(s) that violates unique index.".into(),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [6, null] }, "name": "Keenan" })),
                expected_error: Some(
                    "can not index a doc's field(s) that violates unique index.".into(),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {custom: {numbers: {_any: {_eq: 4}}}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {custom: {numbers: {_any: {_eq: 4}}}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
