//! An import that fails at one document keeps the documents before it,
//! however many of them share a save with the one refused.

use integration_test::actions::{AddCollection, ImportBackup, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.backup.import.keeps_earlier_documents")]
fn a_failed_import_keeps_every_document_before_the_one_refused() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            ImportBackup {
                import_content: r#"{"User":[
                    {"name":"Alice"},
                    {"name":"John"},
                    {"name":"John"},
                    {"name":"Zed"}
                ]}"#
                .into(),
                expected_error: Some("a document with the given ID already exists".into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { User(order: {name: ASC}) { name } }".into(),
                expected_data: json!({ "User": [{ "name": "Alice" }, { "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.backup.import.keeps_earlier_documents")]
fn an_import_refusing_a_value_keeps_every_document_before_it() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        points: Int @crdt(type: pcounter)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            ImportBackup {
                import_content: r#"{"User":[
                    {"name":"Alice","points":1},
                    {"name":"Bob","points":"many"}
                ]}"#
                .into(),
                expected_error: Some("failed to create document in 'User'".into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { User { name } }".into(),
                expected_data: json!({ "User": [{ "name": "Alice" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
