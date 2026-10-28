//! Generated from the conformance scenarios of tests/integration/index/async_backfill_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{IndexField, IndexSpec};
use integration_test::actions::{
    AddCollection, AddDoc, IndexReleaseBuildGate, ListIndexes, NewIndex, NodeOpenIndexGated,
    NodeOpenIndexGatedPersistent, Request, Restart, WaitForIndexReady,
};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.backfill.async")]
fn async_backfill_query_while_building_full_scans() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenIndexGated {
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type User { name: String  age: Int }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 33 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 41 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: false,
                    fields: vec![
                        IndexField {
                            name: "name".into(),
                            descending: false,
                        },
                    ],
                    ..Default::default()
                },
                r#async: true,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query { User(filter: {name: {_eq: "Fred"}}) { age } }"#.into(),
                expected_data: json!({ "User": [{ "age": 33 }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query @explain(type: execute)  { User(filter: {name: {_eq: "Fred"}}) { age } }"#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 0 }))),
                ..Default::default()
            }
            .into(),
            IndexReleaseBuildGate {
                ..Default::default()
            }
            .into(),
            WaitForIndexReady {
                collection: "User".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query @explain(type: execute)  { User(filter: {name: {_eq: "Fred"}}) { age } }"#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.backfill.async")]
fn async_backfill_restart_mid_build_resumes_to_ready() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenIndexGatedPersistent {
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type User { name: String  age: Int }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 33 })),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 41 })),
                common: Common {
                    retry: true,
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
                r#async: true,
                ..Default::default()
            }
            .into(),
            Restart {
                ..Default::default()
            }
            .into(),
            IndexReleaseBuildGate {
                ..Default::default()
            }
            .into(),
            WaitForIndexReady {
                collection: "User".into(),
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
            Request {
                request: r#"query { User(filter: {name: {_eq: "Fred"}}) { age } }"#.into(),
                expected_data: json!({ "User": [{ "age": 33 }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.backfill.async")]
fn async_backfill_while_gated_reports_building_then_ready() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenIndexGated {
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type User { name: String  age: Int }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 33 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 41 })),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: false,
                    fields: vec![
                        IndexField {
                            name: "name".into(),
                            descending: false,
                        },
                    ],
                    ..Default::default()
                },
                r#async: true,
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
                            "Execution": m::object(json!({ "Status": 1, "Action": 3 })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
            IndexReleaseBuildGate {
                ..Default::default()
            }
            .into(),
            WaitForIndexReady {
                collection: "User".into(),
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
            Request {
                request: r#"query { User(filter: {name: {_eq: "Fred"}}) { age } }"#.into(),
                expected_data: json!({ "User": [{ "age": 33 }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query @explain(type: execute)  { User(filter: {name: {_eq: "Fred"}}) { age } }"#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.backfill.async")]
fn async_backfill_write_while_building_reflected_in_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenIndexGated {
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type User { name: String  age: Int }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 33 })),
                common: Common {
                    retry: true,
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
                r#async: true,
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 41 })),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            IndexReleaseBuildGate {
                ..Default::default()
            }
            .into(),
            WaitForIndexReady {
                collection: "User".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query { User(filter: {name: {_eq: "John"}}) { age } }"#.into(),
                expected_data: json!({ "User": [{ "age": 41 }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query { User(filter: {name: {_eq: "Fred"}}) { age } }"#.into(),
                expected_data: json!({ "User": [{ "age": 33 }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
