//! Generated from the conformance scenarios of tests/integration/query/simple/with_variables_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.graphql.variables")]
fn query_simple_with_aggregate_count_variable() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 40 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 21 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query($usersCount: Users__CountSelector) {
                                        COUNT(Users: $usersCount)
                                    }
                "#
                .into(),
                variables: Some(
                    json!({ "usersCount": { "filter": { "Name": { "_eq": "Bob" } } } }),
                ),
                expected_data: json!({ "COUNT": 1 }),
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
#[mantra_macros::req_test("go.query.simple.graphql.variables")]
fn query_simple_with_non_null_variable_returns_error_when_null() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 40 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 21 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query($age: Int!) {
                                        Users(filter: {Age: {_lt: $age}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(
                    r#"Variable "$age" of required type "Int!" was not provided."#.into(),
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
#[mantra_macros::req_test("go.query.simple.graphql.variables")]
fn query_simple_with_non_null_variable() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 40 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 21 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query($age: Int!, $ord: Ordering!) {
                                        Users(filter: {Age: {_lt: $age}}, order: {Age: $ord}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                variables: Some(json!({ "age": 50, "ord": "ASC" })),
                expected_data: json!({ "Users": [{ "Name": "Bob" }, { "Name": "Alice" }] }),
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
#[mantra_macros::req_test("go.query.simple.graphql.variables")]
fn query_simple_with_order_variable() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 40 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 21 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query($order: [UsersOrderArg]) {
                                        Users(order: $order) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                variables: Some(json!({ "order": [{ "Name": "DESC" }, { "Age": "ASC" }] })),
                expected_data: json!({ "Users": [{ "Name": "Bob" }, { "Name": "Alice" }] }),
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
#[mantra_macros::req_test("go.query.simple.graphql.variables")]
fn query_simple_with_variable_default_value_override() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 40 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 21 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query($age: Int = 50) {
                                        Users(filter: {Age: {_lt: $age}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                variables: Some(json!({ "age": 30 })),
                expected_data: json!({ "Users": [{ "Name": "Bob" }] }),
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
#[mantra_macros::req_test("go.query.simple.graphql.variables")]
fn query_simple_with_variable_default_value() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 40 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 21 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query($age: Int = 50, $ord: Ordering = ASC) {
                                        Users(filter: {Age: {_lt: $age}}, order: {Age: $ord}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "Name": "Bob" }, { "Name": "Alice" }] }),
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
