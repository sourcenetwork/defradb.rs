//! Generated from the conformance scenarios of tests/integration/index/json_array_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.json.array")]
fn json_array_index_with_all_eq_and_comparison_filter_should_fetch_correctly_using_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        custom: JSON @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [3, 7] }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [5, [6]] }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [7, 8] }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": 8 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {custom: {numbers: {_all: {_gt: 4}}}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Andy" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {custom: {numbers: {_all: {_gt: 4}}}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 4 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.array")]
fn json_array_index_with_all_eq_filter_should_fetch_correctly_using_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        custom: JSON @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [3, 4] }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [4, [4, 8]] }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": 4 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [4, [4, [4]]] }, "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [4, 4, 4] }, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": 3 }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {custom: {numbers: {_all: {_eq: 4}}}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Bruno" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {custom: {numbers: {_all: {_eq: 4}}}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 4 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.array")]
fn json_array_index_with_any_and_comparison_filter_should_not_consider_them() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        custom: JSON @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [3, 5, 7] }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [0, [6], 4] }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": 5 }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {custom: {numbers: {_any: {_gt: 4}}}}) {
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
                            User(filter: {custom: {numbers: {_any: {_gt: 4}}}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.array")]
fn json_array_index_with_any_eq_filter_should_not_consider_them() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        custom: JSON @index
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [3, 5, [9, 4], 7] }, "name": "John" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [0, [2], 4] }, "name": "Islam" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [3, 5, [1, 0, [9, 4, 6]], 7] }, "name": "Fred" })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [1, 2, [8, 6], 10] }, "name": "Andy" })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": 4 }, "name": "Shahzad" })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
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
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Islam" }] }),
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
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.array")]
fn json_array_index_with_different_element_values_and_types_should_fetch_correctly_using_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        custom: JSON @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [3, 5, 7] }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [3] }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "custom": { "numbers": [4, 8, 4, 4, 5, 4] }, "name": "Shahzad" }),
                ),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [8, null] }, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "custom": { "numbers": [10, "str", true] }, "name": "Bruno" }),
                ),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": 4 }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": null }, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "height": 198 }, "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
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
                expected_data: json!({ "User": [{ "name": "Shahzad" }] }),
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

#[test]
#[mantra_macros::req_test("go.index.json.array")]
fn json_array_index_with_none_eq_and_comparison_filter_should_fetch_correctly_using_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        custom: JSON @index
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [3] }, "name": "John" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [3, 8] }, "name": "Shahzad" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [2, null] }, "name": "Islam" })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [1, [5]] }, "name": "Fred" })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": 5 }, "name": "Andy" })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": null }, "name": "Bruno" })),
                common: Common { capture: captures([("doc0_5", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {custom: {numbers: {_none: {_gt: 4}}}}) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "John" }, { "name": "Fred" }, { "name": "Islam" }] }),
                common: Common { unordered_documents: vec!["/data/User".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {custom: {numbers: {_none: {_gt: 4}}}}) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 0 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.array")]
fn json_array_index_with_none_eq_filter_should_fetch_correctly_using_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        custom: JSON @index
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [3] }, "name": "John" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [4, 8] }, "name": "Shahzad" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [8, null] }, "name": "Islam" })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [1, [4]] }, "name": "Fred" })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": 4 }, "name": "Andy" })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {custom: {numbers: {_none: {_eq: 4}}}}) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "John" }, { "name": "Fred" }, { "name": "Islam" }] }),
                common: Common { unordered_documents: vec!["/data/User".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {custom: {numbers: {_none: {_eq: 4}}}}) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 0 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
