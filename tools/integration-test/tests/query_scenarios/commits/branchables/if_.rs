//! Generated from the conformance scenarios of tests/integration/query/commits/branchables/if_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.branchable.collection-commits")]
fn query_commits_branchables_with_if_directive_false() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users @branchable(if: false) {
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
                        {
                            "cid": m::all(json!([m::same("nameCid"), m::unique("uniqueCid")])),
                        },
                        {
                            "cid": m::all(json!([m::same("ageCid"), m::unique("uniqueCid")])),
                        },
                        {
                            "cid": m::all(json!([m::same("headCid"), m::unique("uniqueCid")])),
                        },
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
fn query_commits_branchables_with_if_directive_true() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users @branchable(if: true) {
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
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "cid": m::all(json!([m::same("collectionCid"), m::unique("uniqueCid")])),
                        },
                        {
                            "cid": m::all(json!([m::same("nameCid"), m::unique("uniqueCid")])),
                        },
                        {
                            "cid": m::all(json!([m::same("ageCid"), m::unique("uniqueCid")])),
                        },
                        {
                            "cid": m::all(json!([m::same("headCid"), m::unique("uniqueCid")])),
                        },
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
