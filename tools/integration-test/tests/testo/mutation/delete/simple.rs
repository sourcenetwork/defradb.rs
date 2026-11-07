//! Generated from the conformance scenarios of tests/integration/mutation/delete/simple_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.mutation.request_validation")]
fn mutation_deletion_without_sub_selection_fields() {
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
                                        delete_User{

                                        }
                                    }
                "#
                .into(),
                expected_error: Some("Syntax Error GraphQL".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.request_validation")]
fn mutation_deletion_without_sub_selection() {
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
                                        delete_User
                                    }
                "#
                .into(),
                expected_error: Some(
                    r#"Field "delete_User" of type "[User]" must have a sub selection."#.into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
