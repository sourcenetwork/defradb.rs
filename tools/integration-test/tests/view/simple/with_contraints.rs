//! Generated from the conformance scenarios of tests/integration/view/simple/with_contraints_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, AddView, RefreshViews, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.view.constraints")]
fn view_simple_with_size_constraint_cache_less_view_does_not_error_on_size_violation() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        pointsListInt: [Int!]
                        pointsListFloat32: [Float32!]
                        pointsListFloat64: [Float64!]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    User {
                        name
                        pointsListInt
                        pointsListFloat32
                        pointsListFloat64
                    }
                "#
                .into(),
                sdl: r#"
                    type UserView @materialized(if: false) {
                        name: String
                        pointsListInt: [Int!] @constraints(size: 2)
                        pointsListFloat32: [Float32!] @constraints(size: 2)
                        pointsListFloat64: [Float64!] @constraints(size: 2)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Alice",
                    "pointsListInt": [1, 2, 3],
                    "pointsListFloat32": [1, 2, 3],
                    "pointsListFloat64": [1, 2, 3],
                })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        UserView {
                            name
                            pointsListInt
                            pointsListFloat32
                            pointsListFloat64
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "UserView": [
                        {
                            "name": "Alice",
                            "pointsListFloat32": [1.0, 2.0, 3.0],
                            "pointsListFloat64": [1.0, 2.0, 3.0],
                            "pointsListInt": [1, 2, 3],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.view.constraints")]
fn view_simple_with_size_constraint_materialized_view_does_not_error_on_size_violation() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        pointsListInt: [Int!]
                        pointsListFloat32: [Float32!]
                        pointsListFloat64: [Float64!]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    User {
                        name
                        pointsListInt
                        pointsListFloat32
                        pointsListFloat64
                    }
                "#
                .into(),
                sdl: r#"
                    type UserView @materialized(if: false) {
                        name: String
                        pointsListInt: [Int!] @constraints(size: 2)
                        pointsListFloat32: [Float32!] @constraints(size: 2)
                        pointsListFloat64: [Float64!] @constraints(size: 2)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Alice",
                    "pointsListInt": [1, 2, 3],
                    "pointsListFloat32": [1, 2, 3],
                    "pointsListFloat64": [1, 2, 3],
                })),
                ..Default::default()
            }
            .into(),
            RefreshViews {
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        UserView {
                            name
                            pointsListInt
                            pointsListFloat32
                            pointsListFloat64
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "UserView": [
                        {
                            "name": "Alice",
                            "pointsListFloat32": [1.0, 2.0, 3.0],
                            "pointsListFloat64": [1.0, 2.0, 3.0],
                            "pointsListInt": [1.0, 2.0, 3.0],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
