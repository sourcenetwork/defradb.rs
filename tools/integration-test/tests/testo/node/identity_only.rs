//! An identity-only node (a `NewNode` that does not sign) keeps its identity
//! but acts as no one, as Go's node does: the transactions it begins and the
//! requests run in them agree on that owner, and node access control checks a
//! request that names no identity as anonymous.

use integration_test::actions::{
    AddCollection, AddDoc, Close, NewNode, Request, Start, TxnBegin, TxnCommit, TxnDiscard,
};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.node.identity_only_transactions")]
fn an_identity_only_node_runs_requests_in_a_transaction_it_begins() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Users { name: String }".into(),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            Request {
                txn: Some(0),
                request: r#"mutation { add_Users(input: [{ name: "John" }]) { _docID } }"#.into(),
                expected_data: json!({ "add_Users": [{ "_docID": m::same("docID") }] }),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { _docID name } }".into(),
                expected_data: json!({
                    "Users": [{ "_docID": m::same("docID"), "name": "John" }],
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
#[mantra_macros::req_test("defra.node.identity_only_nac")]
fn an_identity_only_node_checks_an_anonymous_write_in_its_transaction_under_nac() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                ..Default::default()
            }
            .into(),
            Close {
                keep_data: true,
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Users { name: String }".into(),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            TxnBegin {
                ..Default::default()
            }
            .into(),
            AddDoc {
                txn: Some(0),
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Anonymous" })),
                expected_error: Some("not authorized to perform operation".into()),
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { name } }".into(),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                common: Common {
                    identity_seed: Some(1),
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
