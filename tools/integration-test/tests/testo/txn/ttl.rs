//! Generated from the conformance scenarios of tests/integration/txn/ttl_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, GetCollections, NewNode, Request, TxnBegin, TxnCommit, TxnDiscard, Wait,
};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.txn.ttl")]
fn txn_ttl_given_committed_http_transaction_removes_cache_entry() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                ttl_millis: Some(500),
                tick_millis: Some(50),
                buckets: Some(20),
                ..Default::default()
            }
            .into(),
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
                doc_map: Some(json!({ "name": "John", "age": 27 })),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"
                    mutation {
                                        update_Users(filter: {name: {_eq: "John"}}, input: {age: 28}) {
                                            name
                                            age
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "update_Users": [{ "age": 28, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"
                    query {
                        Users {
                            name
                            age
                        }
                    }
                "#.into(),
                expected_error: Some("transaction not found".into()),
                ..Default::default()
            }
            .into(),
            Wait {
                millis: Some(600),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "age": 28, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.txn.ttl")]
fn txn_ttl_given_discarded_http_transaction_removes_cache_entry() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                ttl_millis: Some(500),
                tick_millis: Some(50),
                buckets: Some(20),
                ..Default::default()
            }
            .into(),
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
                doc_map: Some(json!({ "name": "John", "age": 27 })),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"
                    mutation {
                                        update_Users(filter: {name: {_eq: "John"}}, input: {age: 28}) {
                                            name
                                            age
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "update_Users": [{ "age": 28, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"
                    query {
                        Users {
                            name
                            age
                        }
                    }
                "#.into(),
                expected_error: Some("transaction not found".into()),
                ..Default::default()
            }
            .into(),
            Wait {
                millis: Some(600),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "age": 27, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.txn.ttl")]
fn txn_ttl_given_expired_http_transaction_on_get_collections_returns_non_graph_ql_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                ttl_millis: Some(300),
                tick_millis: Some(50),
                buckets: Some(20),
                ..Default::default()
            }
            .into(),
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
            TxnBegin {
                ..Default::default()
            }
            .into(),
            GetCollections {
                txn: Some(0),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "Name": "Users",
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "PreviousVersion": null,
                            "VersionID": "bafyreihsneodeja4lfer5puptim3lkwvketyckrmkhfpgxm67ch5wenjwq",
                        })),
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Wait {
                millis: Some(900),
                ..Default::default()
            }
            .into(),
            GetCollections {
                txn: Some(0),
                filtered: true,
                expected_error: Some("transaction not found".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.txn.ttl")]
fn txn_ttl_given_http_transaction_activity_extends_ttl() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                ttl_millis: Some(500),
                tick_millis: Some(50),
                buckets: Some(20),
                ..Default::default()
            }
            .into(),
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
                doc_map: Some(json!({ "name": "John", "age": 27 })),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"
                    mutation {
                                        update_Users(filter: {name: {_eq: "John"}}, input: {age: 28}) {
                                            name
                                            age
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "update_Users": [{ "age": 28, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            Wait {
                millis: Some(300),
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"
                    mutation {
                                        update_Users(filter: {name: {_eq: "John"}}, input: {age: 29}) {
                                            name
                                            age
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "update_Users": [{ "age": 29, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            Wait {
                millis: Some(300),
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"
                    query {
                        Users {
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "age": 29, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "age": 29, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.txn.ttl")]
fn txn_ttl_given_idle_http_transaction_discards_transaction() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                ttl_millis: Some(300),
                tick_millis: Some(50),
                buckets: Some(10),
                ..Default::default()
            }
            .into(),
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
                doc_map: Some(json!({ "name": "John", "age": 27 })),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"
                    mutation {
                                        update_Users(filter: {name: {_eq: "John"}}, input: {age: 28}) {
                                            name
                                            age
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "update_Users": [{ "age": 28, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            Wait {
                millis: Some(900),
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"
                    mutation {
                                        update_Users(filter: {name: {_eq: "John"}}, input: {age: 29}) {
                                            name
                                            age
                                        }
                                    }
                "#.into(),
                expected_error: Some("transaction not found".into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "age": 27, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
