//! Generated from the conformance scenarios of tests/integration/backup/simple/import_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, ImportBackup, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.backup.import.documents")]
fn backup_import_empty_object_no_error() {
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
            ImportBackup {
                import_content: r#"{"User":[{}]}"#.into(),
                missing_file: false,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                        User {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": null }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.backup.import.documents")]
fn backup_import_simple_no_error() {
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
            ImportBackup {
                import_content: r#"{"User":[{"_docID":"bae-3fc941b7-505c-5ce2-91a0-b180930ec8a9","_docIDNew":"bae-3fc941b7-505c-5ce2-91a0-b180930ec8a9","age":30,"name":"John"}]}"#.into(),
                missing_file: false,
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
                "#.into(),
                expected_data: json!({ "User": [{ "age": 30, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.backup.import.errors")]
fn backup_import_with_alias_colliding_existing_doc_return_error() {
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
                doc_map: Some(json!({ "name": "John", "age": 30 })),
                ..Default::default()
            }
            .into(),
            ImportBackup {
                import_content: r#"{"User":[{"_docID":"bae-e7a5b940-c466-5e40-af26-87b6b35ed08a","_docIDNew":"bae-e7a5b940-c466-5e40-af26-87b6b35ed08a","age":40,"name":"Bob"}]}"#.into(),
                missing_file: false,
                expected_error: Some("a document with the given ID already exists".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.backup.import.errors")]
fn backup_import_with_doc_already_exists_return_error() {
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
                doc_map: Some(json!({ "name": "John", "age": 30 })),
                ..Default::default()
            }
            .into(),
            ImportBackup {
                import_content: r#"{"User":[{"_docID":"bae-3fc941b7-505c-5ce2-91a0-b180930ec8a9","_docIDNew":"bae-3fc941b7-505c-5ce2-91a0-b180930ec8a9","age":30,"name":"John"}]}"#.into(),
                missing_file: false,
                expected_error: Some("a document with the given ID already exists".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.backup.import.errors")]
fn backup_import_with_invalid_collection_return_error() {
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
            ImportBackup {
                import_content: r#"{"Invalid":[{"_docID":"bae-3fc941b7-505c-5ce2-91a0-b180930ec8a9","_docIDNew":"bae-3fc941b7-505c-5ce2-91a0-b180930ec8a9","age":30,"name":"John"}]}"#.into(),
                missing_file: false,
                expected_error: Some("failed to get collection: collection not found. Name: Invalid".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.backup.import.errors")]
fn backup_import_with_invalid_file_path_return_error() {
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
            ImportBackup {
                import_content: "".into(),
                missing_file: true,
                expected_error: Some("failed to open file".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.backup.import.errors")]
fn backup_import_with_multiple_no_keys_and_invalid_field_errors() {
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
            ImportBackup {
                import_content: r#"{"User":[
					{"age":30,"name":"John"},
					{"INVALID":31,"name":"Smith"},
					{"age":32,"name":"Bob"}
				]}"#
                .into(),
                missing_file: false,
                expected_error: Some("the given field does not exist. Name: INVALID".into()),
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
                expected_data: json!({ "User": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.backup.import.documents")]
fn backup_import_with_multiple_no_keys_no_error() {
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
            ImportBackup {
                import_content: r#"{"User":[
					{"age":30,"name":"John"},
					{"age":31,"name":"Smith"},
					{"age":32,"name":"Bob"}
				]}"#
                .into(),
                missing_file: false,
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
                expected_data: json!({
                    "User": m::unordered(json!([
                        { "age": 31, "name": "Smith" },
                        { "age": 30, "name": "John" },
                        { "age": 32, "name": "Bob" },
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
#[mantra_macros::req_test("go.backup.import.documents")]
fn backup_import_with_no_keys_no_error() {
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
            ImportBackup {
                import_content: r#"{"User":[{"age":30,"name":"John"}]}"#.into(),
                missing_file: false,
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
                expected_data: json!({ "User": [{ "age": 30, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
