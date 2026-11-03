//! Generated from the conformance scenarios of tests/integration/mutation/mix/with_txn_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, TxnBegin, TxnCommit, TxnDiscard};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.txn.isolation")]
fn mutation_with_txn_deletes_user_given_same_transaction() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
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
            Request {
                txn: Some(0),
                request: r#"
                    mutation {
                                        add_User(input: {name: "John", age: 27}) {
                                            _docID
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "add_User": [{ "_docID": "bae-a3a40e95-e33c-52f8-98ef-57fcfc43d8d5" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"
                    mutation {
                                        delete_User(docID: "bae-a3a40e95-e33c-52f8-98ef-57fcfc43d8d5") {
                                            _docID
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "delete_User": [{ "_docID": "bae-a3a40e95-e33c-52f8-98ef-57fcfc43d8d5" }],
                }),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.txn.conflict")]
fn mutation_with_txn_does_not_allow_update_in_second_transaction_user() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 27 })),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
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
                                        update_User(input: {age: 28}) {
                                            _docID
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "update_User": [
                        {
                            "_docID": "bae-a3a40e95-e33c-52f8-98ef-57fcfc43d8d5",
                            "age": 28,
                            "name": "John",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(1),
                request: r#"
                    mutation {
                                        update_User(input: {age: 29}) {
                                            _docID
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "update_User": [
                        {
                            "_docID": "bae-a3a40e95-e33c-52f8-98ef-57fcfc43d8d5",
                            "age": 29,
                            "name": "John",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 1,
                expected_error: Some("transaction conflict. Please retry".into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            _docID
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        {
                            "_docID": "bae-a3a40e95-e33c-52f8-98ef-57fcfc43d8d5",
                            "age": 28,
                            "name": "John",
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
#[mantra_macros::req_test("go.mutation.txn.isolation")]
fn mutation_with_txn_does_not_deletes_user_given_different_transactions() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
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
            Request {
                txn: Some(0),
                request: r#"
                    mutation {
                                        add_User(input: {name: "John", age: 27}) {
                                            _docID
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "add_User": [{ "_docID": "bae-a3a40e95-e33c-52f8-98ef-57fcfc43d8d5" }] }),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(1),
                request: r#"
                    mutation {
                                        delete_User(docID: "bae-a3a40e95-e33c-52f8-98ef-57fcfc43d8d5") {
                                            _docID
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "delete_User": [] }),
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"
                    query {
                                        User {
                                            _docID
                                            name
                                            age
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "_docID": "bae-a3a40e95-e33c-52f8-98ef-57fcfc43d8d5",
                            "age": 27,
                            "name": "John",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(1),
                request: r#"
                    query {
                                        User {
                                            _docID
                                            name
                                            age
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "User": [] }),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 1,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.txn.isolation")]
fn mutation_with_txn_does_not_update_user_given_different_transactions() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 27 })),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
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
                                        update_User(input: {age: 28}) {
                                            _docID
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "update_User": [
                        {
                            "_docID": "bae-a3a40e95-e33c-52f8-98ef-57fcfc43d8d5",
                            "age": 28,
                            "name": "John",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(1),
                request: r#"
                    query {
                                        User {
                                            _docID
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        {
                            "_docID": "bae-a3a40e95-e33c-52f8-98ef-57fcfc43d8d5",
                            "age": 27,
                            "name": "John",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 1,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.txn.isolation")]
fn mutation_with_txn_does_update_user_given_same_transactions() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 27 })),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
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
                                        update_User(input: {age: 28}) {
                                            _docID
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "update_User": [{ "_docID": "bae-a3a40e95-e33c-52f8-98ef-57fcfc43d8d5" }],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"
                    query {
                                        User {
                                            _docID
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        {
                            "_docID": "bae-a3a40e95-e33c-52f8-98ef-57fcfc43d8d5",
                            "age": 28,
                            "name": "John",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
