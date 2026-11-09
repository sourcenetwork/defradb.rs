//! Generated from the conformance scenarios of tests/integration/query/commits/branchables/cid_doc_id_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.branchable.query-args")]
fn query_commits_branchables_with_cid_and_doc_id_param() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users @branchable {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
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
                        r#"query {
						_commits(
							docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#"",
							cid: "bafyreigvb5smrzfjpunohvfsoi3czzjy4dl3cyhzkutth4lx27wnasnqxa"
						) {
							cid
						}
					}"#,
                    ),
                ]),
                expected_error: Some("cid either does not exist or belong to document".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
