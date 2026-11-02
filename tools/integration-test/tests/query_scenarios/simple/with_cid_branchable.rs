//! Generated from the conformance scenarios of tests/integration/query/simple/with_cid_branchable_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.cid.branchable")]
fn query_simple_with_cid_of_branchable_collection_first_cid() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users @branchable {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { _commits(filter: {fieldName: {_eq: null}}, order: {height: DESC}, limit: 1) { cid } }".into(),
                common: Common { capture: captures([("CollectionCID0_0", "/data/_commits/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Freddddd" }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { _commits(filter: {fieldName: {_eq: null}}, order: {height: DESC}, limit: 1) { cid } }".into(),
                common: Common { capture: captures([("CollectionCID0_1", "/data/_commits/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { _commits(filter: {fieldName: {_eq: null}}, order: {height: DESC}, limit: 1) { cid } }".into(),
                common: Common { capture: captures([("CollectionCID0", "/data/_commits/0/cid"), ("CollectionCID0_2", "/data/_commits/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query {
					Users (
							cid: ""#), Text::reference("CollectionCID0_0"), Text::from(r#""
						) {
						name
					}
				}"#)]),
                expected_data: json!({ "Users": [{ "name": "Fred" }] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.cid.branchable")]
fn query_simple_with_cid_of_branchable_collection_last_cid() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users @branchable {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { _commits(filter: {fieldName: {_eq: null}}, order: {height: DESC}, limit: 1) { cid } }".into(),
                common: Common { capture: captures([("CollectionCID0_0", "/data/_commits/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Freddddd" }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { _commits(filter: {fieldName: {_eq: null}}, order: {height: DESC}, limit: 1) { cid } }".into(),
                common: Common { capture: captures([("CollectionCID0_1", "/data/_commits/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { _commits(filter: {fieldName: {_eq: null}}, order: {height: DESC}, limit: 1) { cid } }".into(),
                common: Common { capture: captures([("CollectionCID0", "/data/_commits/0/cid"), ("CollectionCID0_2", "/data/_commits/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query {
					Users (
							cid: ""#), Text::reference("CollectionCID0"), Text::from(r#""
						) {
						name
					}
				}"#)]),
                expected_data: json!({ "Users": [{ "name": "John" }, { "name": "Freddddd" }] }),
                common: Common { compare: Some(true), unordered_documents: vec!["/data/Users".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.cid.branchable")]
fn query_simple_with_cid_of_branchable_collection_middle_cid() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users @branchable {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { _commits(filter: {fieldName: {_eq: null}}, order: {height: DESC}, limit: 1) { cid } }".into(),
                common: Common { capture: captures([("CollectionCID0_0", "/data/_commits/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Freddddd" }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { _commits(filter: {fieldName: {_eq: null}}, order: {height: DESC}, limit: 1) { cid } }".into(),
                common: Common { capture: captures([("CollectionCID0_1", "/data/_commits/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { _commits(filter: {fieldName: {_eq: null}}, order: {height: DESC}, limit: 1) { cid } }".into(),
                common: Common { capture: captures([("CollectionCID0", "/data/_commits/0/cid"), ("CollectionCID0_2", "/data/_commits/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query {
					Users (
							cid: ""#), Text::reference("CollectionCID0_1"), Text::from(r#""
						) {
						name
					}
				}"#)]),
                expected_data: json!({ "Users": [{ "name": "Freddddd" }] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
