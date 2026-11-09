//! Generated from the conformance scenarios of tests/integration/query/commits/branchables/update_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.branchable.collection-commits")]
fn query_commits_branchables_with_doc_update() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users @branchable {
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
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Fred" }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits {
                                                cid
                                                links {
                                                    cid
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
                            "cid": m::all(json!([m::same("collectionUpdateCid"), m::unique("uniqueCid")])),
                            "heads": [{ "cid": m::same("collectionCreateCid") }],
                            "links": [{ "cid": m::same("updateCid") }],
                        },
                        {
                            "cid": m::all(json!([m::same("collectionCreateCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [{ "cid": m::same("createCid") }],
                        },
                        {
                            "cid": m::all(json!([m::same("ageCreateCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("nameUpdateCid"), m::unique("uniqueCid")])),
                            "heads": [{ "cid": m::same("nameCreateCid") }],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("nameCreateCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("updateCid"), m::unique("uniqueCid")])),
                            "heads": [{ "cid": m::same("createCid") }],
                            "links": [{ "cid": m::same("nameUpdateCid") }],
                        },
                        {
                            "cid": m::all(json!([m::same("createCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [
                                { "cid": m::same("ageCreateCid") },
                                { "cid": m::same("nameCreateCid") },
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
