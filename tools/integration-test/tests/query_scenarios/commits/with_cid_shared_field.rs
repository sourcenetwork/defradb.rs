//! Generated from the conformance scenarios of tests/integration/query/commits/with_cid_shared_field_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.cid.lookup")]
fn query_commits_with_shared_field_cid_and_doc_id() {
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
            Request {
                request: Text::concat([
                    Text::from(r#"query { _commits(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"", filter: {fieldName: {_eq: "name"}}) { cid } }"#),
                ]),
                common: Common {
                    capture: captures([("FieldCID0_0_name_0", "/data/_commits/0/cid")]),
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
                    Text::from(r#"query { _commits(docID: ""#),
                    Text::reference("doc0_1"),
                    Text::from(r#"", filter: {fieldName: {_eq: "name"}}) { cid } }"#),
                ]),
                common: Common {
                    capture: captures([("FieldCID0_1_name_0", "/data/_commits/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
						_commits(cid: ""#,
                    ),
                    Text::reference("FieldCID0_0_name_0"),
                    Text::from(r#"", docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#"") {
							cid
							fieldName
							docID
						}
					}"#,
                    ),
                ]),
                expected_data: json!({
                    "_commits": [{ "cid": m::cid(), "docID": m::r("doc0_0"), "fieldName": "name" }],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
						_commits(cid: ""#,
                    ),
                    Text::reference("FieldCID0_1_name_0"),
                    Text::from(r#"", docID: ""#),
                    Text::reference("doc0_1"),
                    Text::from(
                        r#"") {
							cid
							fieldName
							docID
						}
					}"#,
                    ),
                ]),
                expected_data: json!({
                    "_commits": [{ "cid": m::cid(), "docID": m::r("doc0_1"), "fieldName": "name" }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.cid.lookup")]
fn query_commits_with_shared_field_cid_without_doc_id_returns_all_owners() {
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
            Request {
                request: Text::concat([
                    Text::from(r#"query { _commits(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"", filter: {fieldName: {_eq: "name"}}) { cid } }"#),
                ]),
                common: Common {
                    capture: captures([("FieldCID0_0_name_0", "/data/_commits/0/cid")]),
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
						_commits(cid: ""#,
                    ),
                    Text::reference("FieldCID0_0_name_0"),
                    Text::from(
                        r#"") {
							cid
							fieldName
							docID
						}
					}"#,
                    ),
                ]),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::cid(), "docID": m::r("doc0_0"), "fieldName": "name" },
                        { "cid": m::cid(), "docID": m::r("doc0_1"), "fieldName": "name" },
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
