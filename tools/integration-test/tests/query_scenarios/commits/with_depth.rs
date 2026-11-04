//! Generated from the conformance scenarios of tests/integration/query/commits/with_depth_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.depth")]
fn query_commits_with_depth1_and_multiple_docs() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 25 })),
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
                                            _commits(depth: 1) {
                                                cid
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::cid() },
                        { "cid": m::cid() },
                        { "cid": m::cid() },
                        { "cid": m::cid() },
                        { "cid": m::cid() },
                        { "cid": m::cid() },
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
#[mantra_macros::req_test("go.query.commits.depth")]
fn query_commits_with_depth1_with_update() {
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
                request: r#"
                    query {
                                            _commits(depth: 1) {
                                                cid
                                                height
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::cid(), "height": 2 },
                        { "cid": m::cid(), "height": 1 },
                        { "cid": m::cid(), "height": 2 },
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
#[mantra_macros::req_test("go.query.commits.depth")]
fn query_commits_with_depth1() {
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
                                            _commits(depth: 1) {
                                                cid
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [{ "cid": m::cid() }, { "cid": m::cid() }, { "cid": m::cid() }],
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
#[mantra_macros::req_test("go.query.commits.depth")]
fn query_commits_with_depth2_with_update() {
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
            Request {
                request: r#"
                    query {
                                            _commits(depth: 2) {
                                                cid
                                                height
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::cid(), "height": 3 },
                        { "cid": m::cid(), "height": 2 },
                        { "cid": m::cid(), "height": 1 },
                        { "cid": m::cid(), "height": 3 },
                        { "cid": m::cid(), "height": 2 },
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
#[mantra_macros::req_test("go.query.commits.depth")]
fn query_commits_with_filter_field_name_and_depth_returns_commits_at_all_heights() {
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
            Request {
                request: r#"
                    query {
                                            _commits(filter: {fieldName: {_eq: "age"}}, depth: 2) {
                                                fieldName
                                                height
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "fieldName": "age", "height": 3 },
                        { "fieldName": "age", "height": 2 },
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
