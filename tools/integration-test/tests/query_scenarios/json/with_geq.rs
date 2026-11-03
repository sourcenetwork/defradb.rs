//! Generated from the conformance scenarios of tests/integration/query/json/with_geq_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.json.ordering.greater")]
fn query_json_with_greater_equal_filter_with_all_types_should_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Custom: JSON
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Shahzad", "Custom": "32" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Andy", "Custom": [1, 2] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Fred", "Custom": { "one": 1 } })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Custom": false })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "David", "Custom": 32 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Custom: {_geq: 32}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "Name": "David" }] }),
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
#[mantra_macros::req_test("go.query.json.ordering.type-errors")]
fn query_json_with_greater_equal_filter_with_array_value_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Custom: JSON
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Custom": 21 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "David", "Custom": 32 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Custom: {_geq: [1, 2]}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(
                    "unexpected type. Property: condition, Actual: []interface {}".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.json.ordering.type-errors")]
fn query_json_with_greater_equal_filter_with_bool_value_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Custom: JSON
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Custom": 21 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "David", "Custom": 32 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Custom: {_geq: true}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_error: Some("unexpected type. Property: condition, Actual: bool".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.json.ordering.greater")]
fn query_json_with_greater_equal_filter_with_equal_value_should_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Custom: JSON
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Custom": 21 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "David", "Custom": 32 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Custom: {_geq: 32}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "Name": "David" }] }),
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
#[mantra_macros::req_test("go.query.json.ordering.greater")]
fn query_json_with_greater_equal_filter_with_greater_value_should_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Custom: JSON
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Custom": 21 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "David", "Custom": 32 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Custom: {_geq: 31}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "Name": "David" }] }),
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
#[mantra_macros::req_test("go.query.json.ordering.greater")]
fn query_json_with_greater_equal_filter_with_nested_equal_value_should_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Custom: JSON
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Custom": { "age": 21 } })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "David", "Custom": { "age": 32 } })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Custom: {age: {_geq: 32}}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "Name": "David" }] }),
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
#[mantra_macros::req_test("go.query.json.ordering.greater")]
fn query_json_with_greater_equal_filter_with_nested_greater_value_should_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Custom: JSON
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Custom": { "age": 21 } })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "David", "Custom": { "age": 32 } })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Custom: {age: {_geq: 31}}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "Name": "David" }] }),
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
#[mantra_macros::req_test("go.query.json.ordering.null")]
fn query_json_with_greater_equal_filter_with_nested_null_value_should_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Custom: JSON
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Custom": { "age": 21 } })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "David" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Addo", "Custom": { "age": null } })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Custom: {age: {_geq: null}}}) {
                                            Name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "Name": "David" }, { "Name": "John" }, { "Name": "Addo" }] }),
                common: Common { compare: Some(true), unordered_documents: vec!["/data/Users".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.json.ordering.null")]
fn query_json_with_greater_equal_filter_with_null_value_should_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Custom: JSON
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Custom": 21 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "David" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Custom: {_geq: null}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "Name": "David" }, { "Name": "John" }] }),
                common: Common {
                    compare: Some(true),
                    unordered_documents: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.query.json.ordering.type-errors")]
fn query_json_with_greater_equal_filter_with_object_value_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Custom: JSON
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Custom": 21 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "David", "Custom": 32 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Custom: {_geq: {one: 1}}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(
                    "unexpected type. Property: condition, Actual: map[string]interface {}".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.json.ordering.type-errors")]
fn query_json_with_greater_equal_filter_with_string_value_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Custom: JSON
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Custom": 21 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "David", "Custom": 32 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Custom: {_geq: ""}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_error: Some("unexpected type. Property: condition, Actual: string".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
