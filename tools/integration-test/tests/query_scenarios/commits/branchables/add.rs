//! Generated from the conformance scenarios of tests/integration/query/commits/branchables/add_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.branchable.collection-commits")]
fn query_commits_branchables_with_multiple_add() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 25 })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits {
                                                cid
                                                heads {
                                                    cid
                                                }
                                                links {
                                                    cid
                                                }
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "cid": m::all(json!([m::same("doc2CollectionCid"), m::unique("uniqueCid")])),
                            "heads": [{ "cid": m::same("doc1CollectionCid") }],
                            "links": [{ "cid": m::same("doc2CompositeCid") }],
                        },
                        {
                            "cid": m::all(json!([m::same("doc1CollectionCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [{ "cid": m::same("doc1CompositeCid") }],
                        },
                        {
                            "cid": m::all(json!([m::same("doc1AgeFieldCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("doc1NameFieldCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("doc1CompositeCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [
                                { "cid": m::same("doc1AgeFieldCid") },
                                { "cid": m::same("doc1NameFieldCid") },
                            ],
                        },
                        {
                            "cid": m::all(json!([m::same("doc2AgeFieldCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("doc2NameFieldCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("doc2CompositeCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [
                                { "cid": m::same("doc2AgeFieldCid") },
                                { "cid": m::same("doc2NameFieldCid") },
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
