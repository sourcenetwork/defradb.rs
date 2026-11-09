//! Generated from the conformance scenarios of tests/integration/mutation/update/with_filter_action_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::Filter;
use integration_test::actions::{AddCollection, AddDoc, NewNode, Request, UpdateWithFilter};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.update.collection_filter_errors")]
fn update_with_empty_filter_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        heightM: Float
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            UpdateWithFilter {
                collection: "Users".into(),
                filter: Filter {
                    kind: "string".into(),
                    value: Some(json!("")),
                },
                updater: r#"{"name": "Eric"}"#.into(),
                expected_error: Some("filter cannot be empty".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.update.collection_filter")]
fn update_with_filter_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        heightM: Float
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
                ..Default::default()
            }
            .into(),
            UpdateWithFilter {
                collection: "Users".into(),
                filter: Filter {
                    kind: "string".into(),
                    value: Some(json!(r#"{name: {_eq: "John"}}"#)),
                },
                updater: r#"{"name": "Eric"}"#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query{
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Eric" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.update.collection_filter_errors")]
fn update_with_invalid_filter_type_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                ttl_millis: Some(60000),
                tick_millis: Some(1000),
                buckets: Some(60),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        heightM: Float
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            UpdateWithFilter {
                collection: "Users".into(),
                filter: Filter {
                    kind: "structNumber".into(),
                    value: Some(json!({ "Number": 1 })),
                },
                updater: r#"{"name": "Eric"}"#.into(),
                expected_error: Some("collection not found".into()),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(&[ClientType::RustHttp, ClientType::GoHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.update.collection_filter_errors")]
fn update_with_invalid_filter_type_with_go_client_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        heightM: Float
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            UpdateWithFilter {
                collection: "Users".into(),
                filter: Filter {
                    kind: "structNumber".into(),
                    value: Some(json!({ "Number": 1 })),
                },
                updater: r#"{"name": "Eric"}"#.into(),
                expected_error: Some("unsupported filter type".into()),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(&[ClientType::InProcess]),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.update.collection_filter_errors")]
fn update_with_invalid_json_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        heightM: Float
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
                ..Default::default()
            }
            .into(),
            UpdateWithFilter {
                collection: "Users".into(),
                filter: Filter {
                    kind: "string".into(),
                    value: Some(json!(r#"{name: {_eq: "John"}}"#)),
                },
                updater: r#"{name: "Eric"}"#.into(),
                expected_error: Some("cannot parse JSON: cannot parse object".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.update.collection_filter_errors")]
fn update_with_invalid_updater_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        heightM: Float
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
                ..Default::default()
            }
            .into(),
            UpdateWithFilter {
                collection: "Users".into(),
                filter: Filter {
                    kind: "string".into(),
                    value: Some(json!(r#"{name: {_eq: "John"}}"#)),
                },
                updater: r#""name: Eric""#.into(),
                expected_error: Some("the updater of a document is of invalid type".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.update.collection_filter")]
fn update_with_map_filter_large_integer_straddling2_to53_updates_only_exact_match() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        heightM: Float
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 9007199254740992_i64 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 9007199254740993_i64 })),
                ..Default::default()
            }
            .into(),
            UpdateWithFilter {
                collection: "Users".into(),
                filter: Filter {
                    kind: "map".into(),
                    value: Some(json!({ "age": { "_eq": 9007199254740993_i64 } })),
                },
                updater: r#"{"name": "Eric"}"#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query{
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Fred" }, { "name": "Eric" }] }),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(&[
            ClientType::InProcess,
            ClientType::RustHttp,
            ClientType::GoHttp,
            ClientType::Cli,
        ]),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.update.collection_filter")]
fn update_with_map_filter_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        heightM: Float
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
                ..Default::default()
            }
            .into(),
            UpdateWithFilter {
                collection: "Users".into(),
                filter: Filter {
                    kind: "map".into(),
                    value: Some(json!({ "name": { "_eq": "John" } })),
                },
                updater: r#"{"name": "Eric"}"#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query{
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Eric" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.update.collection_filter")]
fn update_with_none_option_filter_updates_all_documents() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        heightM: Float
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
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 25 })),
                ..Default::default()
            }
            .into(),
            UpdateWithFilter {
                collection: "Users".into(),
                filter: Filter {
                    kind: "none".into(),
                    ..Default::default()
                },
                updater: r#"{"verified": true}"#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query{
                                        Users {
                                            name
                                            verified
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "name": "John", "verified": true },
                        { "name": "Fred", "verified": true },
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
#[mantra_macros::req_test("go.mutation.update.collection_filter_errors")]
fn update_with_patch_does_nothing() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        heightM: Float
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
                ..Default::default()
            }
            .into(),
            UpdateWithFilter {
                collection: "Users".into(),
                filter: Filter {
                    kind: "string".into(),
                    value: Some(json!(r#"{name: {_eq: "John"}}"#)),
                },
                updater: r#"[{"name": "Eric"}, {"name": "Sam"}]"#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query{
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.update.collection_filter")]
fn update_with_some_option_filter_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        heightM: Float
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
                ..Default::default()
            }
            .into(),
            UpdateWithFilter {
                collection: "Users".into(),
                filter: Filter {
                    kind: "some".into(),
                    value: Some(json!({ "name": { "_eq": "John" } })),
                },
                updater: r#"{"name": "Eric"}"#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query{
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Eric" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
