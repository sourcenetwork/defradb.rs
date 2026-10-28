//! Generated from the conformance scenarios of tests/integration/query/commits/branchables/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.branchable.collection-commits")]
fn query_commits_branchables() {
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
                request: r#"
                    query {
                                            _commits {
                                                cid
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::unique("uniqueCid") },
                        { "cid": m::unique("uniqueCid") },
                        { "cid": m::unique("uniqueCid") },
                        { "cid": m::unique("uniqueCid") },
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

#[test]
#[mantra_macros::req_test("go.query.commits.branchable.collection-commits")]
fn query_commits_branchables_with_all_fields() {
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
            Request {
                request: r#"
                    query {
                                            _commits {
                                                cid
                                                collectionVersionId
                                                delta
                                                docID
                                                fieldName
                                                height
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
                            "cid": m::all(json!([m::same("collectionCid"), m::unique("uniqueCid")])),
                            "collectionVersionId": "bafyreihsneodeja4lfer5puptim3lkwvketyckrmkhfpgxm67ch5wenjwq",
                            "delta": null,
                            "docID": null,
                            "fieldName": null,
                            "heads": [],
                            "height": 1,
                            "links": [{ "cid": m::same("compositeCid"), "fieldName": "_C" }],
                        },
                        {
                            "cid": m::all(json!([m::same("ageCid"), m::unique("uniqueCid")])),
                            "collectionVersionId": "bafyreihsneodeja4lfer5puptim3lkwvketyckrmkhfpgxm67ch5wenjwq",
                            "delta": "FQ==",
                            "docID": m::r("doc0_0"),
                            "fieldName": "age",
                            "heads": [],
                            "height": 1,
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("nameCid"), m::unique("uniqueCid")])),
                            "collectionVersionId": "bafyreihsneodeja4lfer5puptim3lkwvketyckrmkhfpgxm67ch5wenjwq",
                            "delta": "ZEpvaG4=",
                            "docID": m::r("doc0_0"),
                            "fieldName": "name",
                            "heads": [],
                            "height": 1,
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("compositeCid"), m::unique("uniqueCid")])),
                            "collectionVersionId": "bafyreihsneodeja4lfer5puptim3lkwvketyckrmkhfpgxm67ch5wenjwq",
                            "delta": null,
                            "docID": m::r("doc0_0"),
                            "fieldName": "_C",
                            "heads": [],
                            "height": 1,
                            "links": [
                                { "cid": m::same("ageCid"), "fieldName": "age" },
                                { "cid": m::same("nameCid"), "fieldName": "name" },
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
