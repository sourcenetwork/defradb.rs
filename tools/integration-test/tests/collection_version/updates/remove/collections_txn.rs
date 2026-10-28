//! Generated from the conformance scenarios of tests/integration/collection_version/updates/remove/collections_txn_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, GetCollectionsFiltered, PatchCollection, ScenarioExpectPending,
    TxnBegin, TxnDiscard,
};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.concurrency")]
fn col_version_update_remove_collection_collection_may_be_redeclared_and_used_by_txn() {
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
            TxnBegin {
                ..Default::default()
            }
            .into(),
            AddCollection {
                txn: Some(0),
                sdl: r#"
                    type Books {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "remove",
							"path": "/Users"
						}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                txn: Some(0),
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                txn: Some(0),
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
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
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.concurrency")]
fn col_version_update_remove_collection_get_collections_should_not_return_collection_deleted_whilst_txn_was_open(
) {
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
            TxnBegin {
                ..Default::default()
            }
            .into(),
            AddCollection {
                txn: Some(0),
                sdl: r#"
                    type Books {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "remove",
							"path": "/Users"
						}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollectionsFiltered {
                txn: Some(0),
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "Name": "Books",
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "PreviousVersion": null,
                            "Fields": [
                                m::object(json!({
                                    "Name": "_docID",
                                    "Kind": 1,
                                    "IsPrimary": false,
                                    "Typ": 0,
                                    "DefaultValue": null,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "name",
                                    "Kind": 11,
                                    "IsPrimary": false,
                                    "Typ": 1,
                                    "DefaultValue": null,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                            ],
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
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.with_documents")]
fn col_version_update_remove_collection_with_data_added_in_same_txn() {
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
            TxnBegin {
                ..Default::default()
            }
            .into(),
            AddDoc {
                txn: Some(0),
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                txn: Some(0),
                patch: r#"
						[
							{
								"op": "remove",
								"path": "/Users"
							}
						]
					"#
                .into(),
                expected_error: Some("cannot delete a collection that has documents".into()),
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
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.concurrency")]
fn col_version_update_remove_collection_deadlock_if_deleting_version_with_new_field_whilst_other_txn_writing(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![ScenarioExpectPending {
            children: vec![
                json!({ "op": "node.open", "args": { "node": "db" } }),
                json!({
                    "op": "schema.add",
                    "args": {
                        "node": "db",
                        "sdl": r#"
					type Users {
						name: String
					}
				"#,
                    },
                }),
                json!({
                    "op": "schema.patch",
                    "args": {
                        "node": "db",
                        "patch": r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} }
					]
				"#,
                    },
                }),
                json!({
                    "op": "transaction.begin",
                    "args": { "node": "db", "transaction": "txn0" },
                }),
                json!({
                    "op": "document.create",
                    "args": {
                        "node": "db",
                        "transaction": "txn0",
                        "collection": "Users",
                        "document": { "name": "Fred" },
                    },
                }),
                json!({
                    "op": "schema.patch",
                    "args": {
                        "node": "db",
                        "patch": r#"
					[
						{
							"op": "remove",
							"path": "/bafyreigvzkfdc4y2ppvvpmmdw3t7kv4nd5dgfh5jfytef3kbzem6po55zu"
						}
					]
				"#,
                    },
                }),
            ],
            expected_data: json!(m::object(json!({ "pending": true }))),
            ..Default::default()
        }
        .into()],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.concurrency")]
fn col_version_update_remove_collection_deadlocks_if_other_txn_writing() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![ScenarioExpectPending {
            children: vec![
                json!({ "op": "node.open", "args": { "node": "db" } }),
                json!({
                    "op": "schema.add",
                    "args": {
                        "node": "db",
                        "sdl": r#"
					type Users {
						name: String
					}
				"#,
                    },
                }),
                json!({
                    "op": "transaction.begin",
                    "args": { "node": "db", "transaction": "txn0" },
                }),
                json!({
                    "op": "document.create",
                    "args": {
                        "node": "db",
                        "transaction": "txn0",
                        "collection": "Users",
                        "document": { "name": "Fred" },
                    },
                }),
                json!({
                    "op": "schema.patch",
                    "args": {
                        "node": "db",
                        "patch": r#"
					[
						{
							"op": "remove",
							"path": "/Users"
						}
					]
				"#,
                    },
                }),
            ],
            expected_data: json!(m::object(json!({ "pending": true }))),
            ..Default::default()
        }
        .into()],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
