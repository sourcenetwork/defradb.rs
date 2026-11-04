//! Generated from the conformance scenarios of tests/integration/query/simple/with_version_cid_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.cid.doc-id-match")]
fn query_simple_with_version_and_cid() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query { Users(docID: ""#), Text::reference("doc0_0"), Text::from(r#"") { _version { cid } } }"#)]),
                common: Common { capture: captures([("CID0_0_0", "/data/Users/0/_version/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Chris" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query { Users(docID: ""#), Text::reference("doc0_1"), Text::from(r#"") { _version { cid } } }"#)]),
                common: Common { capture: captures([("CID0_1_0", "/data/Users/0/_version/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query {
					Users(cid: ""#), Text::reference("CID0_1_0"), Text::from(r#"") {
						Name
						_version {
							fieldName
						}
					}
				}"#)]),
                expected_data: json!({ "Users": [{ "Name": "Chris", "_version": [{ "fieldName": "_C" }] }] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
