//! Generated from the conformance scenarios of tests/integration/collection/truncate/view_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, AddView, RefreshViews, Request, Truncate};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection.truncate.views")]
fn truncate_collection_view_add_removes_document() {
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
            AddView {
                query: r#"
                    Users {
                        name
                    }
                "#
                .into(),
                sdl: r#"
                    type UserView @materialized(if: true) {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                ..Default::default()
            }
            .into(),
            RefreshViews {
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "UserView".into(),
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
                expected_data: json!({ "UserView": [] }),
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
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "UserView": m::unordered(json!([{ "name": "John" }, { "name": "Fred" }])),
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
#[mantra_macros::req_test("go.collection.truncate.views")]
fn truncate_collection_view_add_truncating_source_does_not_truncate_view() {
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
            AddView {
                query: r#"
                    Users {
                        name
                    }
                "#
                .into(),
                sdl: r#"
                    type UserView @materialized(if: true) {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                ..Default::default()
            }
            .into(),
            RefreshViews {
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
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
                expected_data: json!({
                    "UserView": m::unordered(json!([{ "name": "John" }, { "name": "Fred" }])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
