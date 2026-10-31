//! Generated from the conformance scenarios of tests/integration/mutation/update/concurrent_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, Parallel, Request, TxnBegin, TxnCommit, TxnDiscard, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.txn.atomic")]
fn mutation_update_concurrent_commit() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
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
            AddDoc {
                collection: "Users".into(),
                txn: Some(0),
                doc_map: Some(json!({ "age": 27, "name": "John" })),
                common: Common { capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Parallel {
                actions: vec![
                        UpdateDoc {
                            collection: "Users".into(),
                            txn: Some(0),
                            doc_id: Text::reference("doc0"),
                            doc_map: json!({ "name": "Fred", "age": 21 }),
                            expected_data: json!(m::object(json!({
                                "error": m::any_of(json!(["", m::substring("transaction not found")])),
                            }))),
                            common: Common { envelope: true, ..Default::default() },
                            ..Default::default()
                        }
                        .into(),
                        TxnCommit {
                            txn: 0,
                            expected_data: json!({ "result": {}, "error": "" }),
                            common: Common { envelope: true, ..Default::default() },
                            ..Default::default()
                        }
                        .into(),
                ],
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
                expected_data: json!({
                    "Users": m::any_of(json!([[{ "name": "John", "age": 27 }], [{ "name": "Fred", "age": 21 }]])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::GoHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.txn.atomic")]
fn mutation_update_concurrent_discard() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                txn: Some(0),
                doc_map: Some(json!({ "age": 27, "name": "John" })),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Parallel {
                actions: vec![
                    AddDoc {
                        collection: "Users".into(),
                        txn: Some(0),
                        doc_map: Some(json!({ "age": 21, "name": "Fred" })),
                        expected_data: json!(m::object(json!({
                            "error": m::any_of(json!(["", m::substring("transaction not found")])),
                        }))),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    TxnDiscard {
                        txn: 0,
                        expected_data: json!({ "result": {}, "error": "" }),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                ],
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
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
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
#[mantra_macros::req_test("go.mutation.txn.atomic")]
fn mutation_update_concurrent_write() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                txn: Some(0),
                doc_map: Some(json!({ "age": 27, "name": "John" })),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Parallel {
                actions: vec![
                    UpdateDoc {
                        collection: "Users".into(),
                        txn: Some(0),
                        doc_id: Text::reference("doc0"),
                        doc_map: json!({ "name": "Fred", "age": 21 }),
                        expected_data: json!({ "error": "", "result": {} }),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                    UpdateDoc {
                        collection: "Users".into(),
                        txn: Some(0),
                        doc_id: Text::reference("doc0"),
                        doc_map: json!({ "name": "Shahzad", "age": 31 }),
                        expected_data: json!({ "error": "", "result": {} }),
                        common: Common {
                            envelope: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                    .into(),
                ],
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
                "#
                .into(),
                expected_data: json!({
                    "Users": m::any_of(json!([
                        [{ "name": "Fred", "age": 21 }],
                        [{ "name": "Shahzad", "age": 31 }],
                    ])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
