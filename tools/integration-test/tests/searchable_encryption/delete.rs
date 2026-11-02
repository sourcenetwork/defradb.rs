//! Generated from the conformance scenarios of tests/integration/searchable_encryption/delete_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, DeleteEncryptedIndex, ListEncryptedIndexes, NewEncryptedIndex, NewNode,
};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.searchable_encryption.index.list_delete")]
fn encrypted_index_delete_after_delete_can_make_new_index_anew() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        age: Int @encryptedIndex
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteEncryptedIndex {
                collection: "User".into(),
                field_name: "age".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListEncryptedIndexes {
                collection: "User".into(),
                expected_data: json!({ "indexes": m::unordered(json!([])) }),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewEncryptedIndex {
                collection: "User".into(),
                field_name: "age".into(),
                index_type: "".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListEncryptedIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([{ "FieldName": "age", "Type": "equality" }])),
                }),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.searchable_encryption.index.list_delete")]
fn encrypted_index_delete_if_index_does_not_exist_return_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        age: Int
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteEncryptedIndex {
                collection: "User".into(),
                field_name: "age".into(),
                expected_error: Some(
                    "encrypted index does not exist on this field. Field: age".into(),
                ),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.searchable_encryption.index.list_delete")]
fn encrypted_index_delete_multiple_indexes_should_only_delete_specified() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @encryptedIndex
                        age: Int @encryptedIndex
                        city: String @encryptedIndex
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListEncryptedIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        { "FieldName": "name", "Type": "equality" },
                        { "FieldName": "age", "Type": "equality" },
                        { "FieldName": "city", "Type": "equality" },
                    ])),
                }),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteEncryptedIndex {
                collection: "User".into(),
                field_name: "age".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListEncryptedIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        { "FieldName": "name", "Type": "equality" },
                        { "FieldName": "city", "Type": "equality" },
                    ])),
                }),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.searchable_encryption.index.list_delete")]
fn encrypted_index_delete_with_existing_index_should_delete_successfully() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        age: Int @encryptedIndex
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListEncryptedIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([{ "FieldName": "age", "Type": "equality" }])),
                }),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteEncryptedIndex {
                collection: "User".into(),
                field_name: "age".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListEncryptedIndexes {
                collection: "User".into(),
                expected_data: json!({ "indexes": m::unordered(json!([])) }),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
