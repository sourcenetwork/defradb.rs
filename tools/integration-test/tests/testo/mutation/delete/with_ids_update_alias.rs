//! Generated from the conformance scenarios of tests/integration/mutation/delete/with_ids_update_alias_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.delete.by_id")]
fn mutation_deletion_with_update_and_i_ds_and_select_alias() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                        points: Float
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "name": "Shahzad", "age": 26, "points": 48.48, "verified": true }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "name": "John", "age": 26, "points": 48.48, "verified": true }),
                ),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "age": 27, "points": 48.2, "verified": false }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"mutation {
					delete_User(docID: [""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(r#"", ""#),
                    Text::reference("doc0_1"),
                    Text::from(
                        r#""]) {
						AliasID: _docID
					}
				}"#,
                    ),
                ]),
                expected_data: json!({
                    "delete_User": [{ "AliasID": m::r("doc0_0") }, { "AliasID": m::r("doc0_1") }],
                }),
                common: Common {
                    unordered_documents: vec!["/data/delete_User".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
