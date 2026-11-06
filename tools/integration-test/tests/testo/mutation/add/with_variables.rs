//! Generated from the conformance scenarios of tests/integration/mutation/add/with_variables_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.add.variables")]
fn mutation_add_with_default_variable() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation($user: [UsersMutationInputArg!] = {name: "Bob"}) {
                                        add_Users(input: $user) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "add_Users": [{ "name": "Bob" }] }),
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
#[mantra_macros::req_test("go.mutation.add.variables")]
fn mutation_add_with_json_variable_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        embed: JSON
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation($embed: JSON) {
                                        add_Users(input: {embed: $embed}) {
                                            embed
                                        }
                                    }
                "#
                .into(),
                variables: Some(json!({ "embed": { "bar": 1 } })),
                expected_data: json!({ "add_Users": [{ "embed": { "bar": 1 } }] }),
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
#[mantra_macros::req_test("go.mutation.add.variables")]
fn mutation_add_with_non_null_variable() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation($user: [UsersMutationInputArg!]!) {
                                        add_Users(input: $user) {
                                            name
                                        }
                                    }
                "#
                .into(),
                variables: Some(json!({ "user": { "name": "Bob" } })),
                expected_data: json!({ "add_Users": [{ "name": "Bob" }] }),
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
#[mantra_macros::req_test("go.mutation.add.variables")]
fn mutation_add_with_variable_in_json_object_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        embed: JSON
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation($message: String) {
                                        add_Users(input: {embed: {message: $message}}) {
                                            embed
                                        }
                                    }
                "#
                .into(),
                variables: Some(json!({ "message": "hello" })),
                expected_data: json!({ "add_Users": [{ "embed": { "message": "hello" } }] }),
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
