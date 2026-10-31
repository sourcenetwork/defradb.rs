//! Generated from the conformance scenarios of tests/integration/searchable_encryption/list_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, ListAllEncryptedIndexes, ListEncryptedIndexes, NewEncryptedIndex, NewNode,
};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.searchable_encryption.index.list_delete")]
fn encrypted_index_list_if_index_added_later_should_return_list_of_existing_indexes() {
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
            ListEncryptedIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([{ "FieldName": "name", "Type": "equality" }])),
                }),
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
                    "indexes": m::unordered(json!([
                        { "FieldName": "name", "Type": "equality" },
                        { "FieldName": "age", "Type": "equality" },
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
fn encrypted_index_list_should_return_list_of_existing_indexes() {
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
                    }

                    type Address {
                        street: String @encryptedIndex
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
                    ])),
                }),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListEncryptedIndexes {
                collection: "Address".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([{ "FieldName": "street", "Type": "equality" }])),
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
fn encrypted_index_list_when_requesting_all_indexes_should_return() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @encryptedIndex
                        age: Int @encryptedIndex
                    }

                    type Address {
                        street: String @encryptedIndex
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ListAllEncryptedIndexes {
                expected_data: json!({
                    "indexes": {
                        "Address": m::unordered(json!([{ "FieldName": "street", "Type": "equality" }])),
                        "User": m::unordered(json!([
                            { "FieldName": "name", "Type": "equality" },
                            { "FieldName": "age", "Type": "equality" },
                        ])),
                    },
                }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
