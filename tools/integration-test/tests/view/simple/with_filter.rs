//! Generated from the conformance scenarios of tests/integration/view/simple/with_filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, AddView, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.view.filter")]
fn view_simple_with_filter_on_view_and_query() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    User(filter: {name: {_eq: "John"}}) {
                        name
                        age
                    }
                "#
                .into(),
                sdl: r#"
                    type UserView @materialized(if: false) {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 31 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 31 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        UserView(filter: {age: {_eq: 31}}) {
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "UserView": [{ "age": 31, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.view.filter")]
fn view_simple_with_filter() {
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
            AddView {
                query: r#"
                    User(filter: {name: {_eq: "John"}}) {
                        name
                    }
                "#
                .into(),
                sdl: r#"
                    type UserView @materialized(if: false) {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        UserView {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "UserView": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
