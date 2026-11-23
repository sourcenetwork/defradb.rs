//! A transaction's outcome is visible to its owner and to nobody else
//! (`defra.txn.outcome-query`).

use integration_test::actions::{
    Actions, AddCollection, TxnBegin, TxnCommit, TxnDiscard, TxnOutcome,
};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

const SDL: &str = "type Users { name: String }";

fn outcome(txn: usize, identity_seed: u64, expected: Value) -> TxnOutcome {
    TxnOutcome {
        txn,
        expected_data: expected,
        common: Common {
            identity_seed: Some(identity_seed),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn outcome_action(txn: usize, identity_seed: u64, expected: Value) -> Actions {
    outcome(txn, identity_seed, expected).into()
}

#[test]
#[mantra_macros::req_test("defra.txn.outcome-query")]
fn an_outcome_is_visible_to_its_owner_and_malformed_ids_are_refused() {
    TestCase {
        client_types: Some(&[ClientType::RustHttp]),
        ..TestCase::from_actions(vec![
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            TxnBegin {
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            outcome_action(0, 1, json!({"outcome": "open"})),
            outcome_action(0, 2, json!({"outcome": "unknown"})),
            TxnCommit {
                txn: 0,
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            outcome_action(0, 1, json!({"outcome": "committed"})),
            outcome_action(0, 2, json!({"outcome": "unknown"})),
            TxnBegin {
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            TxnDiscard {
                txn: 1,
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            outcome_action(1, 1, json!({"outcome": "aborted"})),
            TxnOutcome {
                malformed: true,
                expected_status: Some(400),
                expected_error: Some("invalid transaction id".into()),
                ..Default::default()
            }
            .into(),
        ])
    }
    .execute();
}
