//! Generated from the conformance scenarios of tests/integration/query/simple/with_cid_shared_field_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.cid.doc-version")]
fn query_simple_with_shared_field_cid_returns_all_owners() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Shared", "Age": 21 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { _commits(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"", filter: {fieldName: {_eq: "Name"}}) { cid } }"#),
                ]),
                common: Common {
                    capture: captures([("FieldCID0_0_Name_0", "/data/_commits/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Shared", "Age": 22 })),
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
					Users (cid: ""#,
                    ),
                    Text::reference("FieldCID0_0_Name_0"),
                    Text::from(
                        r#"") {
						_docID
						Name
					}
				}"#,
                    ),
                ]),
                expected_data: json!({
                    "Users": [
                        { "Name": "Shared", "_docID": m::r("doc0_0") },
                        { "Name": "Shared", "_docID": m::r("doc0_1") },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
