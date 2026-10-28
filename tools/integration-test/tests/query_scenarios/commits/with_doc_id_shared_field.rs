//! Generated from the conformance scenarios of tests/integration/query/commits/with_doc_id_shared_field_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.docid")]
fn query_commits_with_doc_id_shared_field_block_across_documents() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shared", "age": 21 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shared", "age": 22 })),
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
                        r#"query {
						_commits(docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#"") {
							fieldName
							docID
						}
					}"#,
                    ),
                ]),
                expected_data: json!({
                    "_commits": [
                        { "docID": m::r("doc0_0"), "fieldName": "name" },
                        { "docID": m::r("doc0_0"), "fieldName": "age" },
                        { "docID": m::r("doc0_0"), "fieldName": "_C" },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/_commits".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
						_commits(docID: ""#,
                    ),
                    Text::reference("doc0_1"),
                    Text::from(
                        r#"") {
							fieldName
							docID
						}
					}"#,
                    ),
                ]),
                expected_data: json!({
                    "_commits": [
                        { "docID": m::r("doc0_1"), "fieldName": "name" },
                        { "docID": m::r("doc0_1"), "fieldName": "age" },
                        { "docID": m::r("doc0_1"), "fieldName": "_C" },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/_commits".into()],
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
