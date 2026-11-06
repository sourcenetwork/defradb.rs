//! Generated from the conformance scenarios of tests/integration/txn/new_index_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{IndexField, IndexSpec};
use integration_test::actions::{
    AddCollection, AddDoc, ListIndexes, NewIndex, TxnBegin, TxnCommit, TxnDiscard,
};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.txn.indexes.read-own-writes")]
fn txn_new_index_exhibits_transactional_isolation_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            TxnBegin {
                ..Default::default()
            }
            .into(),
            AddCollection {
                txn: Some(0),
                sdl: r#"
                    type User {
                        name: String 
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                txn: Some(0),
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common { capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewIndex {
                txn: Some(0),
                collection: "User".into(),
                index: IndexSpec {
                    name: "some_index".into(),
                    unique: false,
                    fields: vec![
                        IndexField {
                            name: "name".into(),
                            descending: false,
                        },
                    ],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "some_index",
                                "ID": 1,
                                "Kind": 0,
                                "KindDescription": {
                                    "Unique": false,
                                    "Fields": m::optional(json!(m::unordered(json!([{ "Name": "name", "Descending": false }])))),
                                },
                                "Fields": m::unordered(json!([{ "Name": "name", "Descending": false }])),
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
#[mantra_macros::req_test("go.txn.indexes.commit-visible")]
fn txn_new_index_with_commit_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common { capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            NewIndex {
                txn: Some(0),
                collection: "User".into(),
                index: IndexSpec {
                    name: "some_index".into(),
                    unique: false,
                    fields: vec![
                        IndexField {
                            name: "name".into(),
                            descending: false,
                        },
                    ],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "some_index",
                                "ID": 1,
                                "Kind": 0,
                                "KindDescription": {
                                    "Unique": false,
                                    "Fields": m::optional(json!(m::unordered(json!([{ "Name": "name", "Descending": false }])))),
                                },
                                "Fields": m::unordered(json!([{ "Name": "name", "Descending": false }])),
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
#[mantra_macros::req_test("go.txn.indexes.uncommitted-invisible")]
fn txn_new_index_without_commit_no_indexes() {
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
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            NewIndex {
                txn: Some(0),
                collection: "User".into(),
                index: IndexSpec {
                    name: "some_index".into(),
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
                expected_data: json!({ "indexes": m::unordered(json!([])) }),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
