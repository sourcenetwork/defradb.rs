//! Generated from the conformance scenarios of tests/integration/mutation/mix/with_multiple_operations_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.ordering")]
fn mutation_multiple_operations_execute_in_request_order() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        a1: add_User(input: {name: "Alice"}) { name }
                                        a2: add_User(input: {name: "Bob"}) { name }
                                        a3: add_User(input: {name: "Carol"}) { name }
                                        a4: add_User(input: {name: "Dave"}) { name }
                                        a5: add_User(input: {name: "Eve"}) { name }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "a1": [{ "name": "Alice" }],
                    "a2": [{ "name": "Bob" }],
                    "a3": [{ "name": "Carol" }],
                    "a4": [{ "name": "Dave" }],
                    "a5": [{ "name": "Eve" }],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        { "name": "Alice" },
                        { "name": "Bob" },
                        { "name": "Carol" },
                        { "name": "Dave" },
                        { "name": "Eve" },
                    ],
                }),
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
#[mantra_macros::req_test("go.mutation.ordering")]
fn mutation_multiple_operations_via_fragment_spread_execute_in_request_order() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        ...AddFirst
                                        second: add_User(input: {name: "Second"}) { name }
                                    }
                                    fragment AddFirst on Mutation {
                                        first: add_User(input: {name: "First"}) { name }
                                    }
                "#.into(),
                expected_data: json!({ "first": [{ "name": "First" }], "second": [{ "name": "Second" }] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "First" }, { "name": "Second" }] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
