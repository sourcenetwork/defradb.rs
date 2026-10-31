//! Generated from the conformance scenarios of tests/integration/index/backfill_unique_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{IndexField, IndexSpec};
use integration_test::actions::{
    AddCollection, AddDoc, DeleteIndex, ListIndexes, NewIndex, Request, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.unique.backfill")]
fn unique_index_backfill_with_duplicate_values_fails_and_persists_definition() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age:  Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice", "age": 21 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 21 })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: true,
                    fields: vec![
                        IndexField {
                            name: "age".into(),
                            descending: false,
                        },
                    ],
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
                                "Name": "User_age_ASC",
                                "ID": 1,
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
            Request {
                request: r#"
                    query {
                            User(filter: {age: {_eq: 21}}) {
                                name
                                age
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [{ "age": 21, "name": "Bob" }, { "age": 21, "name": "Alice" }] }),
                common: Common { unordered_documents: vec!["/data/User".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {age: {_eq: 21}}) {
                                name
                                age
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 0 }))),
                ..Default::default()
            }
            .into(),
            DeleteIndex {
                collection: "User".into(),
                index_name: "User_age_ASC".into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({ "indexes": m::unordered(json!([])) }),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_1"),
                doc_map: json!({ "age": 22 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: true,
                    fields: vec![
                        IndexField {
                            name: "age".into(),
                            descending: false,
                        },
                    ],
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
                                "Name": "User_age_ASC",
                                "ID": 2,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "age", "Descending": false })),
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
