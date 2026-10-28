//! Generated from the conformance scenarios of tests/integration/txn/get_collections_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, GetCollectionsFiltered, TxnBegin, TxnDiscard};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.txn.definitions.same-txn-visibility")]
fn txn_get_collections_inside_txn_with_add_schema_succeeds() {
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
            GetCollectionsFiltered {
                txn: Some(0),
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
#[mantra_macros::req_test("go.txn.definitions.same-txn-visibility")]
fn txn_get_collections_inside_txn_without_add_schema_no_collections() {
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
            TxnBegin {
                ..Default::default()
            }
            .into(),
            GetCollectionsFiltered {
                txn: Some(1),
                expected_data: json!({ "collections": [] }),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 1,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
