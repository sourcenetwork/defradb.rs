//! Generated from the conformance scenarios of tests/integration/backup/self_reference/export_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, ExportBackup, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.backup.export.content")]
fn backup_export_multiple_docs_and_doc_update_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                        boss: User @primary @relation(name: "boss_minion")
                        minion: User @relation(name: "boss_minion")
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
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 31, "boss": m::r("doc0_0"), "name": "Bob" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "age": 31 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            ExportBackup {
                format: "".into(),
                pretty: false,
                collections: vec![],
                missing_file: false,
                expected_data: json!({
                    "content": m::concat([json!(r#"{"User":[{"_docID":""#), m::r("doc0_0"), json!(r#"","_docIDNew":""#), m::r("doc0_0"), json!(r#"","age":31,"name":"John"},{"_bossID":""#), m::r("doc0_0"), json!(r#"","_docID":""#), m::r("doc0_1"), json!(r#"","_docIDNew":""#), m::r("doc0_1"), json!(r#"","age":31,"name":"Bob"}]}"#)]),
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
fn backup_export_simple_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                        boss: User @primary @relation(name: "boss_minion")
                        minion: User @relation(name: "boss_minion")
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
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 31, "boss": m::r("doc0_0"), "name": "Bob" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ExportBackup {
                format: "".into(),
                pretty: false,
                collections: vec!["User".into()],
                missing_file: false,
                expected_data: json!({
                    "content": m::concat([json!(r#"{"User":[{"_docID":""#), m::r("doc0_0"), json!(r#"","_docIDNew":""#), m::r("doc0_0"), json!(r#"","age":30,"name":"John"},{"_bossID":""#), m::r("doc0_0"), json!(r#"","_docID":""#), m::r("doc0_1"), json!(r#"","_docIDNew":""#), m::r("doc0_1"), json!(r#"","age":31,"name":"Bob"}]}"#)]),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
