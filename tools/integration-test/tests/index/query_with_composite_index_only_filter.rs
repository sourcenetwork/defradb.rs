//! Generated from the conformance scenarios of tests/integration/index/query_with_composite_index_only_filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.query.composite-prefix")]
fn query_with_composite_index_if_consecutive_eq_ops_should_use_all_to_optimize_query() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "age"}, {field: "numChildren"}]) {
                        name: String
                        age: Int
                        numChildren: Int
                        about: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 22, "numChildren": 2, "about": "bob1" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 22, "numChildren": 2, "about": "bob2" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 22, "numChildren": 0, "about": "bob3" })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 44, "numChildren": 2, "about": "bob4" })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice", "age": 22, "numChildren": 2, "about": "alice" })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                User(filter: {name: {_eq: "Bob"}}) {
                                    about
                                }
                            }
                "#.into(),
                expected_data: json!({
                    "User": [
                        { "about": "bob3" },
                        { "about": "bob1" },
                        { "about": "bob2" },
                        { "about": "bob4" },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                User(filter: {name: {_eq: "Bob"}}) {
                                    about
                                }
                            }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 4 }))),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                User(filter: {name: {_eq: "Bob"}, age: {_eq: 22}}) {
                                    about
                                }
                            }
                "#.into(),
                expected_data: json!({
                    "User": [{ "about": "bob3" }, { "about": "bob1" }, { "about": "bob2" }],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                User(filter: {name: {_eq: "Bob"}, age: {_eq: 22}}) {
                                    about
                                }
                            }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 3 }))),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                User(filter: {name: {_eq: "Bob"}, age: {_eq: 22}, numChildren: {_eq: 2}}) {
                                    about
                                }
                            }
                "#.into(),
                expected_data: json!({ "User": [{ "about": "bob1" }, { "about": "bob2" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                User(filter: {name: {_eq: "Bob"}, age: {_eq: 22}, numChildren: {_eq: 2}}) {
                                    about
                                }
                            }
                "#.into(),
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
#[mantra_macros::req_test("go.index.query.composite-prefix")]
fn query_with_composite_index_if_first_field_is_not_in_filter_should_not_use_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "age"}]) {
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
                doc_map: Some(
                    json!({ "name": "Shahzad", "age": 20, "email": "shahzad@gmail.com" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "age": 23, "email": "bruno@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "age": 44, "email": "roy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 28, "email": "fred@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30, "email": "john@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 32, "email": "islam@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 33, "email": "andy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 42, "email": "addo@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "age": 48, "email": "keenan@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "age": 55, "email": "chris@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        User(filter: {age: {_eq: 32}}) {
                                                name
                                            }
                                        }
                "#
                .into(),
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
#[mantra_macros::req_test("go.index.query.composite-prefix")]
fn query_with_composite_index_if_middle_field_is_not_in_filter_should_ignore_value() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "email"}, {field: "age"}]) {
                        name: String
                        email: String
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice", "email": "alice@gmail.com", "age": 22 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alan", "email": "alan@gmail.com", "age": 38 })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "email": "bob@gmail.com", "age": 51 })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User(filter: {name: {_like: "%l%"}, age: {_gt: 30}}) {
                            name
                        }
                    }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Alan" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_equal_filter_on_nil_value_on_first_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "age"}]) {
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
                doc_map: Some(json!({ "name": "Alice", "age": 22 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 32 })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User(filter: {name: {_eq: null}}) {
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "age": 32, "name": null }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_equal_filter_on_nil_value_on_second_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "age"}]) {
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
                doc_map: Some(json!({ "name": "Alice", "age": 22 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
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
                        User(filter: {name: {_eq: "Alice"}, age: {_eq: null}}) {
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "age": null, "name": "Alice" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_equal_filter_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "age"}]) {
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
                doc_map: Some(
                    json!({ "name": "Shahzad", "age": 20, "email": "shahzad@gmail.com" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "age": 23, "email": "bruno@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "age": 44, "email": "roy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 28, "email": "fred@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30, "email": "john@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 32, "email": "islam@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 33, "email": "andy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 42, "email": "addo@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "age": 48, "email": "keenan@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "age": 55, "email": "chris@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_eq: "Islam"}}) {
                                name
                                age
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "age": 32, "name": "Islam" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {name: {_eq: "Islam"}}) {
                                name
                                age
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_eq: "Islam"}, age: {_eq: 32}}) {
                                name
                                age
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "age": 32, "name": "Islam" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {name: {_eq: "Islam"}, age: {_eq: 32}}) {
                                name
                                age
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_eq: "Islam"}, age: {_eq: 66}}) {
                                name
                                age
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_greater_or_equal_filter_on_first_field_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "age"}, {field: "name"}]) {
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
                doc_map: Some(
                    json!({ "name": "Shahzad", "age": 20, "email": "shahzad@gmail.com" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "age": 23, "email": "bruno@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "age": 44, "email": "roy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 28, "email": "fred@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30, "email": "john@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 32, "email": "islam@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 33, "email": "andy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 42, "email": "addo@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "age": 48, "email": "keenan@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "age": 55, "email": "chris@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_neq: "Keenan"}, age: {_geq: 44},}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Roy" }, { "name": "Chris" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {name: {_neq: "Keenan"}, age: {_geq: 44},}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 3 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_greater_or_equal_filter_on_second_field_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "age"}]) {
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
                doc_map: Some(
                    json!({ "name": "Shahzad", "age": 20, "email": "shahzad@gmail.com" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "age": 23, "email": "bruno@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "age": 44, "email": "roy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 28, "email": "fred@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30, "email": "john@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 32, "email": "islam@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 33, "email": "andy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 42, "email": "addo@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "age": 48, "email": "keenan@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "age": 55, "email": "chris@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {age: {_geq: 44}, name: {_neq: "Keenan"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Chris" }, { "name": "Roy" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {age: {_geq: 44}, name: {_neq: "Keenan"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 10 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_greater_than_filter_on_first_field_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "age"}, {field: "name"}]) {
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
                doc_map: Some(
                    json!({ "name": "Shahzad", "age": 20, "email": "shahzad@gmail.com" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "age": 23, "email": "bruno@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "age": 44, "email": "roy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 28, "email": "fred@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30, "email": "john@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 32, "email": "islam@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 33, "email": "andy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 42, "email": "addo@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "age": 48, "email": "keenan@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "age": 55, "email": "chris@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_neq: "Keenan"}, age: {_gt: 44}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Chris" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {name: {_neq: "Keenan"}, age: {_gt: 44}}) {
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
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_greater_than_filter_on_second_field_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "age"}]) {
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
                doc_map: Some(
                    json!({ "name": "Shahzad", "age": 20, "email": "shahzad@gmail.com" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "age": 23, "email": "bruno@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "age": 44, "email": "roy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 28, "email": "fred@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30, "email": "john@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 32, "email": "islam@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 33, "email": "andy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 42, "email": "addo@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "age": 48, "email": "keenan@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "age": 55, "email": "chris@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_neq: "Keenan"}, age: {_gt: 44}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Chris" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {name: {_neq: "Keenan"}, age: {_gt: 44}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 10 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_in_filter_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "age"}]) {
                        name: String
                        age: Int
                        email: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 20, "email": "shahzad@gmail.com" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "age": 23, "email": "bruno@gmail.com" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "age": 44, "email": "roy@gmail.com" })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 28, "email": "fred@gmail.com" })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30, "email": "john@gmail.com" })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 32, "email": "islam@gmail.com" })),
                common: Common { capture: captures([("doc0_5", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 33, "email": "andy@gmail.com" })),
                common: Common { capture: captures([("doc0_6", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 42, "email": "addo@gmail.com" })),
                common: Common { capture: captures([("doc0_7", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "age": 48, "email": "keenan@gmail.com" })),
                common: Common { capture: captures([("doc0_8", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "age": 55, "email": "chris@gmail.com" })),
                common: Common { capture: captures([("doc0_9", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {age: {_in: [20, 28, 33]}, name: {_in: ["Addo", "Andy", "Fred"]}}) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Andy" }, { "name": "Fred" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {age: {_in: [20, 28, 33]}, name: {_in: ["Addo", "Andy", "Fred"]}}) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 3 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_less_or_equal_filter_on_first_field_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "age"}, {field: "name"}]) {
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
                doc_map: Some(
                    json!({ "name": "Shahzad", "age": 20, "email": "shahzad@gmail.com" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "age": 23, "email": "bruno@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "age": 44, "email": "roy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 28, "email": "fred@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30, "email": "john@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 32, "email": "islam@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 33, "email": "andy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 42, "email": "addo@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "age": 48, "email": "keenan@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "age": 55, "email": "chris@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {age: {_leq: 28}, name: {_neq: "Bruno"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Shahzad" }, { "name": "Fred" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {age: {_leq: 28}, name: {_neq: "Bruno"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 3 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_less_or_equal_filter_on_second_field_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "age"}]) {
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
                doc_map: Some(
                    json!({ "name": "Shahzad", "age": 20, "email": "shahzad@gmail.com" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "age": 23, "email": "bruno@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "age": 44, "email": "roy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 28, "email": "fred@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30, "email": "john@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 32, "email": "islam@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 33, "email": "andy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 42, "email": "addo@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "age": 48, "email": "keenan@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "age": 55, "email": "chris@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {age: {_leq: 28}, name: {_neq: "Bruno"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Fred" }, { "name": "Shahzad" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {age: {_leq: 28}, name: {_neq: "Bruno"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 10 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_less_than_filter_on_first_field_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "age"}, {field: "name"}]) {
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
                doc_map: Some(
                    json!({ "name": "Shahzad", "age": 20, "email": "shahzad@gmail.com" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "age": 23, "email": "bruno@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "age": 44, "email": "roy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 28, "email": "fred@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30, "email": "john@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 32, "email": "islam@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 33, "email": "andy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 42, "email": "addo@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "age": 48, "email": "keenan@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "age": 55, "email": "chris@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {age: {_lt: 24}, name: {_neq: "Shahzad"}}) {
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
                            User(filter: {age: {_lt: 24}, name: {_neq: "Shahzad"}}) {
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
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_less_than_filter_on_second_field_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "age"}]) {
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
                doc_map: Some(
                    json!({ "name": "Shahzad", "age": 20, "email": "shahzad@gmail.com" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "age": 23, "email": "bruno@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "age": 44, "email": "roy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 28, "email": "fred@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30, "email": "john@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 32, "email": "islam@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 33, "email": "andy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 42, "email": "addo@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "age": 48, "email": "keenan@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "age": 55, "email": "chris@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {age: {_lt: 24}, name: {_neq: "Shahzad"}}) {
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
                            User(filter: {age: {_lt: 24}, name: {_neq: "Shahzad"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 10 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_like_filter_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "email"}]) {
                        name: String 
                        email: String 
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "email": "shahzad@gmail.com" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "email": "bruno@gmail.com" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "email": "roy@gmail.com" })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "email": "fred@gmail.com" })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "email": "john@gmail.com" })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "email": "islam@gmail.com" })),
                common: Common { capture: captures([("doc0_5", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "email": "andy@gmail.com" })),
                common: Common { capture: captures([("doc0_6", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "email": "addo@gmail.com" })),
                common: Common { capture: captures([("doc0_7", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "email": "keenan@gmail.com" })),
                common: Common { capture: captures([("doc0_8", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "email": "chris@gmail.com" })),
                common: Common { capture: captures([("doc0_9", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                User(filter: {email: {_like: "a%"}, name: {_like: "%o"}}) {
                                    name
                                }
                            }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Addo" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                User(filter: {email: {_like: "a%"}, name: {_like: "%o"}}) {
                                    name
                                }
                            }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 10 }))),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                User(filter: {email: {_like: "%d@gmail.com"}, name: {_like: "F%"}}) {
                                    name
                                }
                            }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Fred" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                User(filter: {email: {_like: "%d@gmail.com"}, name: {_like: "F%"}}) {
                                    name
                                }
                            }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 10 }))),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                User(filter: {email: {_like: "%e%"}, name: {_like: "%n%"}}) {
                                    name
                                }
                            }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Keenan" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                User(filter: {email: {_like: "%e%"}, name: {_like: "%n%"}}) {
                                    name
                                }
                            }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 10 }))),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {email: {_like: "fred@gmail.com"}, name: {_like: "Fred"}}) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Fred" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {email: {_like: "fred@gmail.com"}, name: {_like: "Fred"}}) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 10 }))),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {email: {_like: "a%@gmail.com"}, name: {_like: "%dd%"}}) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Addo" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {email: {_like: "a%@gmail.com"}, name: {_like: "%dd%"}}) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 10 }))),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {email: {_like: "a%com%m"}}) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {email: {_like: "s%"}, name: {_like: "s%h%d"}}) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_not_equal_filter_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "age"}]) {
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
                doc_map: Some(
                    json!({ "name": "Shahzad", "age": 20, "email": "shahzad@gmail.com" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "age": 23, "email": "bruno@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "age": 44, "email": "roy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 28, "email": "fred@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30, "email": "john@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 32, "email": "islam@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 33, "email": "andy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 42, "email": "addo@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "age": 48, "email": "keenan@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "age": 55, "email": "chris@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_neq: "Islam"}, age: {_neq: 28}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        { "name": "Addo" },
                        { "name": "Andy" },
                        { "name": "Bruno" },
                        { "name": "Chris" },
                        { "name": "John" },
                        { "name": "Keenan" },
                        { "name": "Roy" },
                        { "name": "Shahzad" },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {name: {_neq: "Islam"}, age: {_neq: 28}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 10 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_not_in_filter_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "age"}]) {
                        name: String
                        age: Int
                        email: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 20, "email": "shahzad@gmail.com" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "age": 23, "email": "bruno@gmail.com" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "age": 44, "email": "roy@gmail.com" })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 28, "email": "fred@gmail.com" })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30, "email": "john@gmail.com" })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 32, "email": "islam@gmail.com" })),
                common: Common { capture: captures([("doc0_5", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 33, "email": "andy@gmail.com" })),
                common: Common { capture: captures([("doc0_6", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 42, "email": "addo@gmail.com" })),
                common: Common { capture: captures([("doc0_7", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "age": 48, "email": "keenan@gmail.com" })),
                common: Common { capture: captures([("doc0_8", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "age": 55, "email": "chris@gmail.com" })),
                common: Common { capture: captures([("doc0_9", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {age: {_nin: [20, 23, 28, 42]}, name: {_nin: ["John", "Andy", "Chris"]}}) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Islam" }, { "name": "Keenan" }, { "name": "Roy" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {age: {_nin: [20, 23, 28, 42]}, name: {_nin: ["John", "Andy", "Chris"]}}) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 10 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.composite-operators")]
fn query_with_composite_index_with_not_like_filter_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "email"}]) {
                        name: String 
                        email: String 
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "email": "shahzad@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "email": "bruno@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy", "email": "roy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "email": "fred@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "email": "john@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "email": "islam@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "email": "andy@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "email": "addo@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "email": "keenan@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris", "email": "chris@gmail.com" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_nlike: "%h%"}, email: {_nlike: "%d%"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        { "name": "Bruno" },
                        { "name": "Islam" },
                        { "name": "Keenan" },
                        { "name": "Roy" },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {name: {_nlike: "%h%"}, email: {_nlike: "%d%"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 10 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
