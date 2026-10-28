//! Generated from the conformance scenarios of tests/integration/mutation/update/with_delete_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, DeleteDoc, SaveDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.deleted_doc")]
fn update_save_deleted_doc_does_nothing() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            SaveDoc {
                collection: "Users".into(),
                doc_map: json!({ "name": "John" }),
                common: Common {
                    capture: captures([("docID", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("docID"),
                ..Default::default()
            }
            .into(),
            SaveDoc {
                collection: "Users".into(),
                doc_id: Some(Text::reference("docID")),
                doc_map: json!({ "name": "Fred" }),
                expected_error: Some("a document with the given ID has been deleted".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
