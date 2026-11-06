//! Generated from the conformance scenarios of tests/integration/mutation/special/invalid_operation_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.mutation.request_validation")]
fn mutation_invalid_mutation() {
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
                                        dostuff_User(input: {}) {
                                            _docID
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(
                    r#"Cannot query field "dostuff_User" on type "Mutation"."#.into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
