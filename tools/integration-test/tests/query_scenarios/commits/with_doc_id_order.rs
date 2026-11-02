//! Generated from the conformance scenarios of tests/integration/query/commits/with_doc_id_order_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.order")]
fn query_commits_with_doc_id_and_order_and_multi_updates_cid_asc() {
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
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "age": 23 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "age": 24 }),
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
                        r#"", order: {height: ASC}) {
							 cid
							 height
						 }
					 }"#,
                    ),
                ]),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::cid(), "height": 1 },
                        { "cid": m::cid(), "height": 1 },
                        { "cid": m::cid(), "height": 1 },
                        { "cid": m::cid(), "height": 2 },
                        { "cid": m::cid(), "height": 2 },
                        { "cid": m::cid(), "height": 3 },
                        { "cid": m::cid(), "height": 3 },
                        { "cid": m::cid(), "height": 4 },
                        { "cid": m::cid(), "height": 4 },
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
#[mantra_macros::req_test("go.query.commits.order")]
fn query_commits_with_doc_id_and_order_cid_asc() {
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
                        r#"", order: {cid: ASC}) {
							cid
							height
						}
					}"#,
                    ),
                ]),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::cid(), "height": 1 },
                        { "cid": m::cid(), "height": 1 },
                        { "cid": m::cid(), "height": 2 },
                        { "cid": m::cid(), "height": 2 },
                        { "cid": m::cid(), "height": 1 },
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
#[mantra_macros::req_test("go.query.commits.order")]
fn query_commits_with_doc_id_and_order_cid_desc() {
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
                        r#"", order: {cid: DESC}) {
							cid
							height
						}
					}"#,
                    ),
                ]),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::cid(), "height": 1 },
                        { "cid": m::cid(), "height": 2 },
                        { "cid": m::cid(), "height": 2 },
                        { "cid": m::cid(), "height": 1 },
                        { "cid": m::cid(), "height": 1 },
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
#[mantra_macros::req_test("go.query.commits.order")]
fn query_commits_with_doc_id_and_order_height_asc() {
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
                        r#"", order: {height: ASC}) {
							cid
							height
						}
					}"#,
                    ),
                ]),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::cid(), "height": 1 },
                        { "cid": m::cid(), "height": 1 },
                        { "cid": m::cid(), "height": 1 },
                        { "cid": m::cid(), "height": 2 },
                        { "cid": m::cid(), "height": 2 },
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
#[mantra_macros::req_test("go.query.commits.order")]
fn query_commits_with_doc_id_and_order_height_desc() {
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
                        r#"", order: {height: DESC}) {
							cid
							height
						}
					}"#,
                    ),
                ]),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::cid(), "height": 2 },
                        { "cid": m::cid(), "height": 2 },
                        { "cid": m::cid(), "height": 1 },
                        { "cid": m::cid(), "height": 1 },
                        { "cid": m::cid(), "height": 1 },
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
