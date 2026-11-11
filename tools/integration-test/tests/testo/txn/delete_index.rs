//! Generated from the conformance scenarios of tests/integration/txn/delete_index_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, DeleteIndex, ListIndexes, TxnBegin, TxnCommit, TxnDiscard,
};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.txn.indexes.read-own-writes")]
fn txn_delete_index_exhibits_transactional_isolation_succeeds() {
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
                        name: String @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            DeleteIndex {
                txn: Some(0),
                collection: "User".into(),
                index_name: "User_name_ASC".into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                txn: Some(0),
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

#[test]
#[mantra_macros::req_test("go.txn.indexes.commit-visible")]
fn txn_delete_index_with_commit_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            DeleteIndex {
                txn: Some(0),
                collection: "User".into(),
                index_name: "User_name_ASC".into(),
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
                expected_data: json!({ "indexes": m::unordered(json!([])) }),
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
fn txn_delete_index_without_commit_does_not_delete() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            DeleteIndex {
                txn: Some(0),
                collection: "User".into(),
                index_name: "User_name_ASC".into(),
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
