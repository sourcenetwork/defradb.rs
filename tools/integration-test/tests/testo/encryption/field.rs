//! Generated from the conformance scenarios of tests/integration/encryption/field_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.encryption.field.errors")]
fn doc_encryption_field_if_attempt_to_encrypt_builtin_field_in_gql_schema_return_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_Users(input: {age: 21, name: "John"}, encryptFields: [_docID]) { _docID } }"#.into(),
                expected_error: Some(r#"Argument "encryptFields" has invalid value [_docID]."#.into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_Users(input: {age: 21, name: "John"}, encryptFields: [_version]) { _docID } }"#.into(),
                expected_error: Some(r#"Argument "encryptFields" has invalid value [_version]."#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.encryption.field.errors")]
fn doc_encryption_field_if_attempt_to_encrypt_builtin_field_return_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(false),
                encrypted_fields: Some(vec!["_docID".into()]),
                expected_error: Some("can not encrypt build-in field. Name: _docID".into()),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(false),
                encrypted_fields: Some(vec!["_version".into()]),
                expected_error: Some("the given field does not exist. Name: _version".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.encryption.field.errors")]
fn doc_encryption_field_if_field_does_not_exist_in_gql_schema_return_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_Users(input: {age: 21, name: "John"}, encryptFields: [points]) { _docID } }"#.into(),
                expected_error: Some(r#"Argument "encryptFields" has invalid value [points]."#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.encryption.field.errors")]
fn doc_encryption_field_if_field_does_not_exist_return_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(false),
                encrypted_fields: Some(vec!["points".into()]),
                expected_error: Some("the given field does not exist. Name: points".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
