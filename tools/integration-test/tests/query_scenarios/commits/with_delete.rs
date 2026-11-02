//! Generated from the conformance scenarios of tests/integration/query/commits/with_delete_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, DeleteDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.listing")]
fn query_commits_after_doc_deletion_should_still_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                expected_data: json!({ "deleted": true }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits(filter: {fieldName: {_eq: "_C"}}) {
                            cid
                            fieldName
                            links {
                                cid
                                fieldName
                            }
                            heads {
                                cid
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "cid": m::all(json!([m::same("deleteCid"), m::unique("uniqueCid")])),
                            "fieldName": "_C",
                            "heads": [{ "cid": m::same("createCompositeCid") }],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("createCompositeCid"), m::unique("uniqueCid")])),
                            "fieldName": "_C",
                            "heads": [],
                            "links": [
                                { "cid": m::cid(), "fieldName": "age" },
                                { "cid": m::cid(), "fieldName": "name" },
                            ],
                        },
                    ],
                }),
                common: Common { unordered_documents: vec!["/data/_commits".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
