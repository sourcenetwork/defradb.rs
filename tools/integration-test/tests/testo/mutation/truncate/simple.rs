//! Generated from the conformance scenarios of tests/integration/mutation/truncate/simple_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, TxnBegin, TxnDiscard};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.truncate.apply")]
fn mutation_truncate() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: "mutation { truncate_User }".into(),
                expected_data: json!({ "truncate_User": true }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { User { name } }".into(),
                expected_data: json!({ "User": [] }),
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
#[mantra_macros::req_test("go.mutation.truncate.restrictions")]
fn mutation_truncate_must_be_standalone() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String }".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        truncate_User
                                        add_User(input: {name: "Alice"}) { name }
                                    }
                "#
                .into(),
                expected_error: Some(
                    "truncate mutation must be the only field in an operation".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.truncate.restrictions")]
fn mutation_truncate_cannot_run_in_transaction() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String }".into(),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: "mutation { truncate_User }".into(),
                expected_error: Some("truncate mutation cannot run in a transaction".into()),
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
