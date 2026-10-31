//! Generated from the conformance scenarios of tests/integration/mutation/update/with_id_and_version_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.update.by_id")]
fn mutation_update_with_id_and_version_return_results() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        points: Float
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": 42.1 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Bob", "points": 66.6 })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"mutation {
					update_Users(docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#"", input: {points: 59}) {
						name
						_version {
							cid
						}
					}
				}"#,
                    ),
                ]),
                expected_data: json!({
                    "update_Users": [
                        {
                            "_version": [
                                { "cid": m::same("updateCid") },
                                { "cid": m::same("createCid") },
                            ],
                            "name": "John",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
