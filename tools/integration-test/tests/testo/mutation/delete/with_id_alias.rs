//! Generated from the conformance scenarios of tests/integration/mutation/delete/with_id_alias_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.delete.by_id")]
fn mutation_deletion_with_id_and_alias() {
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
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
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
                    Text::from(
                        r#""]) {
						fancyKey: _docID
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "delete_User": [{ "fancyKey": m::r("doc0_0") }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
