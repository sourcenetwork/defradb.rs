//! Generated from the conformance scenarios of tests/integration/collection_version/with_inline_array_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.define.basic_types")]
fn collection_version_inline_array_adds_collection_given_second_type() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        favouriteIntegers: [Int!]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Books {
                        pageNumbers: [Int!]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type (name: "Books") {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "__type": { "name": "Books" } }),
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
#[mantra_macros::req_test("go.collection_version.define.basic_types")]
fn collection_version_inline_array_adds_collection_given_single_type() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        favouriteIntegers: [Int!]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type (name: "Users") {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "__type": { "name": "Users" } }),
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
