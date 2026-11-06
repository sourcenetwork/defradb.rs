//! Generated from the conformance scenarios of tests/integration/query/json/with_any_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.json.array-ops")]
fn query_json_with_any_filter_and_nested_array_should_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                                        name: String
                                        custom: JSON
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({
                    "name": "Shahzad",
                    "custom": [1, false, "second", { "one": 1 }, [1, 2]],
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({
                    "name": "Fred",
                    "custom": [null, false, "second", { "one": 1 }, [1, [2, 3]]],
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Islam", "custom": null })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Keenan", "custom": 3 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Bruno", "custom": [null, 3] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Andy", "custom": "" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "custom": true })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {custom: {_any: {_eq: 3}}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Bruno" }] }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.json.array-ops")]
fn query_json_with_any_filter_with_all_types_should_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                                        name: String
                                        custom: JSON
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({
                    "name": "Shahzad",
                    "custom": [1, false, "second", { "one": 1 }, [1, 2]],
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({
                    "name": "Fred",
                    "custom": [null, false, "second", { "one": 1 }, [1, 2]],
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Islam", "custom": null })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Keenan", "custom": 0 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Andy", "custom": "" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "custom": true })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {custom: {_any: {_eq: null}}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Fred" }] }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
