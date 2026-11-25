//! Backup import re-creates a document under its new genesis's ID, and keeps
//! the ID the file names as an alias of it (`defra.document.id.import_alias`).
//! The ID stays the file's only when the importing node rebuilds the same
//! genesis (same signer, collection version and field values); `FILE_ID` is
//! no genesis's ID, so here the stored `_docID` cannot be it.

use integration_test::actions::{AddCollection, ImportBackup, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

const FILE_ID: &str = "bae-00000000-0000-5000-8000-000000000001";

#[test]
#[mantra_macros::req_test("defra.document.id.import_alias")]
fn an_imported_document_gets_a_new_id_and_its_file_id_resolves_to_it() {
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
                import_content: format!(
                    r#"{{"User":[{{"_docID":"{FILE_ID}","_docIDNew":"{FILE_ID}","age":30,"name":"John"}}]}}"#
                )
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { User { _docID name } }".into(),
                expected_data: json!({
                    "User": [{ "_docID": m::all(json!([m::doc_id(), m::not(json!(FILE_ID)), m::same("new")])), "name": "John" }],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: format!(r#"query {{ User(docID: "{FILE_ID}") {{ _docID name }} }}"#).into(),
                expected_data: json!({ "User": [{ "_docID": m::same("new"), "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
