//! Generated from the conformance scenarios of tests/integration/txn/add_collection_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, GetCollectionsFiltered, TxnBegin, TxnCommit, TxnDiscard,
};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.txn.definitions.commit-visible")]
fn txn_add_collection_with_commit_succeeds() {
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
                    type Users {
                        name: String
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            GetCollectionsFiltered {
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "Name": "Users",
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "PreviousVersion": null,
                            "VersionID": "bafyreihsneodeja4lfer5puptim3lkwvketyckrmkhfpgxm67ch5wenjwq",
                        })),
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
#[mantra_macros::req_test("go.txn.definitions.uncommitted-invisible")]
fn txn_add_collection_without_commit_empty_results() {
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
                    type Users {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollectionsFiltered {
                expected_data: json!({ "collections": [] }),
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
