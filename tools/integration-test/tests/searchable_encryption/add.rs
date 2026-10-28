//! Generated from the conformance scenarios of tests/integration/searchable_encryption/add_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, NewEncryptedIndex, NewNode, Request};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.searchable_encryption.index.create")]
fn encrypted_index_new_after_add_request_should_not_hinder_querying() {
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
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("doc0", "/docID")]),
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
            Request {
                request: r#"
                    query  {
                        User {
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "age": 21, "name": "John" }] }),
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
#[mantra_macros::req_test("go.searchable_encryption.index.create")]
fn encrypted_index_new_if_index_already_exists_should_return_error() {
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
            NewEncryptedIndex {
                collection: "User".into(),
                field_name: "age".into(),
                index_type: "".into(),
                expected_error: Some(
                    "encrypted index already exists on this field. Field: age".into(),
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
#[mantra_macros::req_test("go.searchable_encryption.index.create")]
fn encrypted_index_new_if_non_existent_field_is_given_return_error() {
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
            NewEncryptedIndex {
                collection: "User".into(),
                field_name: "verified".into(),
                index_type: "".into(),
                expected_error: Some(
                    "encrypted index on non-existent field. Field: verified".into(),
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
#[mantra_macros::req_test("go.searchable_encryption.index.create")]
fn encrypted_index_new_schema_with_encrypted_index_should_not_hinder_querying() {
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
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                        User {
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "age": 21, "name": "John" }] }),
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
