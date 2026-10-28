//! Generated from the conformance scenarios of tests/integration/query/commits/with_nested_links_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.links.structure")]
fn query_commits_with_single_add_nested_links_composite_filter_succeed() {
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
                        _commits(filter: {fieldName: {_eq: "_C"}}) {
                            height
                            fieldName
                            links {
                                height
                                fieldName
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "fieldName": "_C",
                            "height": 1,
                            "links": m::unordered(json!([
                                m::object(json!({ "fieldName": "age", "height": 1 })),
                                m::object(json!({ "fieldName": "name", "height": 1 })),
                            ])),
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
#[mantra_macros::req_test("go.query.commits.links.structure")]
fn query_commits_with_single_add_nested_links_nested_filter_succeed() {
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
                        _commits(filter: {fieldName: {_eq: "_C"}}) {
                            height
                            fieldName
                            links(filter: {fieldName: {_eq: "age"}}) {
                                height
                                fieldName
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "fieldName": "_C",
                            "height": 1,
                            "links": [{ "fieldName": "age", "height": 1 }],
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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

#[test]
#[mantra_macros::req_test("go.query.commits.links.structure")]
fn query_commits_with_single_add_nested_links_succeed() {
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
                            height
                            fieldName
                            links {
                                cid
                                height
                                fieldName
                            }
                            heads {
                                cid
                                height
                            }	
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "cid": m::same("ageCreateCid"),
                            "fieldName": "age",
                            "heads": [],
                            "height": 1,
                            "links": [],
                        },
                        {
                            "cid": m::same("nameCreateCid"),
                            "fieldName": "name",
                            "heads": [],
                            "height": 1,
                            "links": [],
                        },
                        {
                            "cid": m::same("createCompositeCid"),
                            "fieldName": "_C",
                            "heads": [],
                            "height": 1,
                            "links": m::unordered(json!([
                                m::object(json!({ "cid": m::cid(), "fieldName": "age", "height": 1 })),
                                m::object(json!({ "cid": m::cid(), "fieldName": "name", "height": 1 })),
                            ])),
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
#[mantra_macros::req_test("go.query.commits.links.structure")]
fn query_commits_with_single_update_double_nested_links_succeeds() {
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
                doc_map: json!({ "age": 22 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            cid
                            delta
                            docID
                            fieldName
                            height
                            links {
                                fieldName
                                cid
                                height
                                docID
                                heads {
                                    fieldName
                                    cid
                                }
                            }
                            heads {
                                cid
                                height
                                docID
                                links {
                                    fieldName
                                    cid
                                }
                            }

                        }
                    }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "cid": m::all(json!([m::same("ageUpdateCid"), m::unique("uniqueCid")])),
                            "delta": "Fg==",
                            "docID": m::r("doc0_0"),
                            "fieldName": "age",
                            "heads": [
                                {
                                    "cid": m::same("ageCreateCid"),
                                    "docID": m::r("doc0_0"),
                                    "height": 1,
                                    "links": [],
                                },
                            ],
                            "height": 2,
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("ageCreateCid"), m::unique("uniqueCid")])),
                            "delta": "FQ==",
                            "docID": m::r("doc0_0"),
                            "fieldName": "age",
                            "heads": [],
                            "height": 1,
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("nameCreateCid"), m::unique("uniqueCid")])),
                            "delta": "ZEpvaG4=",
                            "docID": m::r("doc0_0"),
                            "fieldName": "name",
                            "heads": [],
                            "height": 1,
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("updateCompositeCid"), m::unique("uniqueCid")])),
                            "delta": null,
                            "docID": m::r("doc0_0"),
                            "fieldName": "_C",
                            "heads": [
                                {
                                    "cid": m::same("createCompositeCid"),
                                    "docID": m::r("doc0_0"),
                                    "height": 1,
                                    "links": m::unordered(json!([
                                        m::object(json!({ "cid": m::cid(), "fieldName": "age" })),
                                        m::object(json!({ "cid": m::cid(), "fieldName": "name" })),
                                    ])),
                                },
                            ],
                            "height": 2,
                            "links": [
                                {
                                    "cid": m::same("ageUpdateCid"),
                                    "docID": m::r("doc0_0"),
                                    "fieldName": "age",
                                    "heads": [
                                        {
                                            "cid": m::same("ageCreateCid"),
                                            "fieldName": "age",
                                        },
                                    ],
                                    "height": 2,
                                },
                            ],
                        },
                        {
                            "cid": m::all(json!([m::same("createCompositeCid"), m::unique("uniqueCid")])),
                            "delta": null,
                            "docID": m::r("doc0_0"),
                            "fieldName": "_C",
                            "heads": [],
                            "height": 1,
                            "links": m::unordered(json!([
                                m::object(json!({ "cid": m::cid(), "fieldName": "age", "height": 1 })),
                                m::object(json!({ "cid": m::cid(), "fieldName": "name", "height": 1 })),
                            ])),
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
