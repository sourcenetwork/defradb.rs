//! Two documents of one backup file that share a `_docID`: the second fails
//! with the alias error and is not created, however the import batches.

use integration_test::actions::{AddCollection, ImportBackup, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.backup.import.duplicate_alias")]
fn a_repeated_doc_id_fails_and_keeps_the_first_document() {
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
                    {"_docID":"bae-3fc941b7-505c-5ce2-91a0-b180930ec8a9","name":"John","age":30},
                    {"_docID":"bae-3fc941b7-505c-5ce2-91a0-b180930ec8a9","name":"Bob","age":40}
                ]}"#
                .into(),
                expected_error: Some("a document with the given ID already exists".into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { User { name } }".into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
