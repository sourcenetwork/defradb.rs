//! Generated from the conformance scenarios of tests/integration/collection/truncate/index_filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, Truncate};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection.truncate.filter")]
fn truncate_collection_index_filter_removes_document() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String @index
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
            Truncate {
                collection: "Users".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {name: {_eq: "John"}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection.truncate.filter")]
fn truncate_collection_index_filter_with_unique_index_allows_recreation_of_document() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String @index(unique: true)
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
            Truncate {
                collection: "Users".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {name: {_eq: "John"}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection.truncate.filter")]
fn truncate_collection_index_filter_with_unique_index_removes_document() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String @index(unique: true)
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
            Truncate {
                collection: "Users".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {name: {_eq: "John"}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
