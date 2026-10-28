//! Generated from the conformance scenarios of tests/integration/query/commits/with_doc_id_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.docid")]
fn query_commits_doc_id_empty_list() {
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
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits(docID: []) {
                                                cid
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({ "_commits": [] }),
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
#[mantra_macros::req_test("go.query.commits.docid")]
fn query_commits_doc_id_list_of_many() {
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
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
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
						_commits(docID: [""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(r#"", ""#),
                    Text::reference("doc0_1"),
                    Text::from(
                        r#""]) {
							cid
						}
					}"#,
                    ),
                ]),
                expected_error: Some("querying by multiple docIDs is not yet supported".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.docid")]
fn query_commits_doc_id_list_of_one() {
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
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
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
						_commits(docID: [""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#""]) {
							cid
						}
					}"#,
                    ),
                ]),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::unique("inline_unique_1") },
                        { "cid": m::unique("inline_unique_2") },
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
#[mantra_macros::req_test("go.query.commits.docid")]
fn query_commits_with_doc_id() {
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
                request: Text::concat([
                    Text::from(
                        r#"query {
						_commits(docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#"") {
							cid
						}
					}"#,
                    ),
                ]),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::unique("uniqueCid") },
                        { "cid": m::unique("uniqueCid") },
                        { "cid": m::unique("uniqueCid") },
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

#[test]
#[mantra_macros::req_test("go.query.commits.links.structure")]
fn query_commits_with_doc_id_and_links() {
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
                request: Text::concat([Text::from(r#"query {
						_commits(docID: ""#), Text::reference("doc0_0"), Text::from(r#"") {
							cid
							links {
								cid
								fieldName
							}
							heads {
								cid
							}
						}
					}"#)]),
                expected_data: json!({
                    "_commits": [
                        {
                            "cid": m::all(json!([m::same("ageCreateCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("nameCreateCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("createCompositeCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [
                                { "cid": m::same("ageCreateCid"), "fieldName": "age" },
                                { "cid": m::same("nameCreateCid"), "fieldName": "name" },
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

#[test]
#[mantra_macros::req_test("go.query.commits.links.structure")]
fn query_commits_with_doc_id_and_update_and_links() {
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
                request: Text::concat([Text::from(r#"query {
						_commits(docID: ""#), Text::reference("doc0_0"), Text::from(r#"") {
							cid
							links {
								cid
								fieldName
							}
							heads {
								cid
							}
						}
					}"#)]),
                expected_data: json!({
                    "_commits": [
                        {
                            "cid": m::all(json!([m::same("ageUpdateCid"), m::unique("uniqueCid")])),
                            "heads": [{ "cid": m::same("ageCreateCid") }],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("ageCreateCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("nameCreateCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("updateCompositeCid"), m::unique("uniqueCid")])),
                            "heads": [{ "cid": m::same("createCompositeCid") }],
                            "links": [{ "cid": m::same("ageUpdateCid"), "fieldName": "age" }],
                        },
                        {
                            "cid": m::all(json!([m::same("createCompositeCid"), m::unique("uniqueCid")])),
                            "heads": [],
                            "links": [
                                { "cid": m::same("ageCreateCid"), "fieldName": "age" },
                                { "cid": m::same("nameCreateCid"), "fieldName": "name" },
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

#[test]
#[mantra_macros::req_test("go.query.commits.docid")]
fn query_commits_with_doc_id_and_update() {
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
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "age": 22 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
						_commits(docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#"") {
							cid
							height
						}
					}"#,
                    ),
                ]),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::unique("uniqueCid"), "height": 2 },
                        { "cid": m::unique("uniqueCid"), "height": 1 },
                        { "cid": m::unique("uniqueCid"), "height": 1 },
                        { "cid": m::unique("uniqueCid"), "height": 2 },
                        { "cid": m::unique("uniqueCid"), "height": 1 },
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

#[test]
#[mantra_macros::req_test("go.query.commits.docid")]
fn query_commits_with_unknown_doc_id() {
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
                                            _commits(docID: "unknown document ID") {
                                                cid
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({ "_commits": [] }),
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
