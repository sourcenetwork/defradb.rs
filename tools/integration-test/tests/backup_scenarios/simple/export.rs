//! Generated from the conformance scenarios of tests/integration/backup/simple/export_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, ExportBackup};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.backup.export.content")]
fn backup_export_empty_no_error() {
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
                doc_map: Some(json!({})),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ExportBackup {
                format: "".into(),
                pretty: false,
                collections: vec![],
                missing_file: false,
                expected_data: json!({
                    "content": m::concat([json!(r#"{"User":[{"_docID":""#), m::r("doc0_0"), json!(r#"","_docIDNew":""#), m::r("doc0_0"), json!(r#""}]}"#)]),
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
#[mantra_macros::req_test("go.backup.export.collections")]
fn backup_export_just_user_collection_no_error() {
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
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ExportBackup {
                format: "".into(),
                pretty: false,
                collections: vec!["User".into()],
                missing_file: false,
                expected_data: json!({
                    "content": m::concat([json!(r#"{"User":[{"_docID":""#), m::r("doc0_0"), json!(r#"","_docIDNew":""#), m::r("doc0_0"), json!(r#"","age":30,"name":"John"}]}"#)]),
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
#[mantra_macros::req_test("go.backup.export.content")]
fn backup_export_simple_no_error() {
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
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ExportBackup {
                format: "".into(),
                pretty: false,
                collections: vec![],
                missing_file: false,
                expected_data: json!({
                    "content": m::concat([json!(r#"{"User":[{"_docID":""#), m::r("doc0_0"), json!(r#"","_docIDNew":""#), m::r("doc0_0"), json!(r#"","age":30,"name":"John"}]}"#)]),
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
#[mantra_macros::req_test("go.backup.export.errors")]
fn backup_export_with_invalid_collection_return_error() {
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
                doc_map: Some(json!({ "name": "John", "age": 30 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ExportBackup {
                format: "".into(),
                pretty: false,
                collections: vec!["Invalid".into()],
                missing_file: false,
                expected_error: Some(
                    "failed to get collection: collection not found. Name: Invalid".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.backup.export.errors")]
fn backup_export_with_invalid_file_path_return_error() {
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
                doc_map: Some(json!({ "name": "John", "age": 30 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ExportBackup {
                format: "".into(),
                pretty: false,
                collections: vec![],
                missing_file: true,
                expected_error: Some("failed to create file".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
