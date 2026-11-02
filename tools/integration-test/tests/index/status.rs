//! Generated from the conformance scenarios of tests/integration/index/status_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{IndexField, IndexSpec};
use integration_test::actions::{AddCollection, AddDoc, ListIndexes, NewIndex};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.definition.status")]
fn index_status_new_index_is_ready() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age:  Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: false,
                    fields: vec![IndexField {
                        name: "name".into(),
                        descending: false,
                    }],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "User_name_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "name", "Descending": false })),
                                ])),
                            })),
                            "Execution": m::object(json!({ "Status": 3, "Action": 0 })),
                        })),
                    ])),
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
#[mantra_macros::req_test("go.index.definition.status")]
fn index_status_ready_and_failed_in_one_collection_each_reports_own_status() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age:  Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice", "age": 21 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 21 })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: false,
                    fields: vec![IndexField {
                        name: "name".into(),
                        descending: false,
                    }],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: true,
                    fields: vec![IndexField {
                        name: "age".into(),
                        descending: false,
                    }],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "User_name_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "name", "Descending": false })),
                                ])),
                            })),
                            "Execution": m::object(json!({ "Status": 3, "Action": 0 })),
                        })),
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "User_age_ASC",
                                "ID": 2,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "age", "Descending": false })),
                                ])),
                            })),
                            "Execution": m::object(json!({
                                "Status": 2,
                                "Action": 3,
                                "Reason": m::substring("can not index"),
                            })),
                        })),
                    ])),
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
#[mantra_macros::req_test("go.index.definition.status")]
fn index_status_sdl_index_is_ready() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                        age:  Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "User_name_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "name", "Descending": false })),
                                ])),
                            })),
                            "Execution": m::object(json!({ "Status": 3, "Action": 0 })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
