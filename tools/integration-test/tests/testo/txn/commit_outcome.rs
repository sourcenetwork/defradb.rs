//! Every failed commit says whether it committed (`defra.txn.commit_outcome`).

use integration_test::actions::{
    Actions, AddCollection, AddDoc, ArmCommitFault, CommitFault, NewNode, Request, Restart,
    TxnBegin, TxnCommit, TxnDiscard,
};
use integration_test::m;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

const SDL: &str = "type Users { name: String }";

/// The clients that run a Rust node in this process: the fault env is an env
/// in the node's process, and Go has no outcome field.
const RUST_CLIENTS: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli];

/// The clients that send the commit over HTTP: only they can resend it.
const HTTP_CLIENTS: &[ClientType] = &[ClientType::RustHttp, ClientType::Cli];

fn faulty_node() -> Actions {
    NewNode {
        commit_fault: true,
        ..Default::default()
    }
    .into()
}

fn add_collection() -> Actions {
    AddCollection {
        sdl: SDL.into(),
        ..Default::default()
    }
    .into()
}

fn add_in(txn: usize, name: &str) -> Actions {
    AddDoc {
        txn: Some(txn),
        collection: "Users".into(),
        doc_map: Some(json!({ "name": name })),
        ..Default::default()
    }
    .into()
}

fn arm_io() -> Actions {
    ArmCommitFault {
        fault: CommitFault::Io,
        ..Default::default()
    }
    .into()
}

fn users(expected: serde_json::Value) -> Actions {
    Request {
        request: "query { Users { name } }".into(),
        expected_data: json!({ "Users": expected }),
        ..Default::default()
    }
    .into()
}

fn rename_alice(txn: usize, name: &str) -> Actions {
    Request {
        txn: Some(txn),
        request: format!(
            r#"mutation {{ update_Users(filter: {{name: {{_eq: "Alice"}}}}, input: {{name: "{name}"}}) {{ name }} }}"#
        )
        .into(),
        expected_data: json!({ "update_Users": [{ "name": name }] }),
        ..Default::default()
    }
    .into()
}

/// The transaction's one document is either there or not: never half of it.
fn all_or_nothing(name: &str) -> Actions {
    users(m::any_of(json!([[], [{ "name": name }]])))
}

#[test]
#[mantra_macros::req_test("defra.txn.commit_outcome.committed")]
fn an_acknowledged_commit_reads_back_after_restart() {
    TestCase {
        client_types: Some(RUST_CLIENTS),
        ..TestCase::from_actions(vec![
            NewNode {
                database_dir: Some("".into()),
                ..Default::default()
            }
            .into(),
            add_collection(),
            TxnBegin::default().into(),
            add_in(0, "Shahzad"),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            users(json!([{ "name": "Shahzad" }])),
            Restart::default().into(),
            users(json!([{ "name": "Shahzad" }])),
        ])
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.commit_outcome.not_committed")]
#[mantra_macros::req_test("defra.txn.commit_outcome.cli_exit")]
fn a_conflict_refusal_leaves_nothing_and_reports_aborted() {
    TestCase {
        client_types: Some(RUST_CLIENTS),
        ..TestCase::from_actions(vec![
            add_collection(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                ..Default::default()
            }
            .into(),
            TxnBegin::default().into(),
            TxnBegin::default().into(),
            rename_alice(0, "Bob"),
            rename_alice(1, "Carol"),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 1,
                expected_error: Some("transaction conflict. Please retry".into()),
                expected_commit_outcome: Some("aborted".into()),
                expected_exit_code: Some(3),
                ..Default::default()
            }
            .into(),
            users(json!([{ "name": "Bob" }])),
        ])
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.commit_outcome.indeterminate")]
#[mantra_macros::req_test("defra.txn.commit_outcome.cli_exit")]
fn an_injected_io_failure_at_commit_reports_indeterminate() {
    TestCase {
        client_types: Some(RUST_CLIENTS),
        ..TestCase::from_actions(vec![
            faulty_node(),
            add_collection(),
            TxnBegin::default().into(),
            add_in(0, "Shahzad"),
            arm_io(),
            TxnCommit {
                txn: 0,
                expected_error: Some("Input/output error".into()),
                expected_commit_outcome: Some("indeterminate".into()),
                expected_exit_code: Some(4),
                ..Default::default()
            }
            .into(),
            Restart::default().into(),
            all_or_nothing("Shahzad"),
        ])
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.commit_outcome.no_resend")]
fn an_indeterminate_commit_is_not_resent() {
    TestCase {
        client_types: Some(HTTP_CLIENTS),
        ..TestCase::from_actions(vec![
            faulty_node(),
            add_collection(),
            TxnBegin::default().into(),
            add_in(0, "Shahzad"),
            arm_io(),
            // A resend finds no transaction and answers `transaction not
            // found`: the caller would never see the I/O error.
            TxnCommit {
                txn: 0,
                expected_error: Some("Input/output error".into()),
                ..Default::default()
            }
            .into(),
        ])
    }
    .execute();
}

/// GraphQL answers a commit failure with HTTP 200, so no client resends it:
/// this checks what the node leaves, not the client's resend rule.
#[test]
#[mantra_macros::req_test("defra.txn.commit_outcome.indeterminate")]
fn an_autocommit_write_that_fails_at_commit_is_all_or_nothing() {
    TestCase {
        client_types: Some(RUST_CLIENTS),
        ..TestCase::from_actions(vec![
            faulty_node(),
            add_collection(),
            arm_io(),
            Request {
                request: r#"mutation { add_Users(input: {name: "Shahzad"}) { name } }"#.into(),
                expected_error: Some("Input/output error".into()),
                ..Default::default()
            }
            .into(),
            Restart::default().into(),
            all_or_nothing("Shahzad"),
        ])
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.commit_outcome.no_open_txn")]
#[mantra_macros::req_test("defra.txn.commit_outcome.cli_exit")]
fn a_commit_on_a_finished_transaction_reports_retained_status() {
    TestCase {
        client_types: Some(RUST_CLIENTS),
        ..TestCase::from_actions(vec![
            add_collection(),
            TxnBegin::default().into(),
            TxnDiscard {
                txn: 0,
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                expected_error: Some("transaction not found".into()),
                expected_commit_outcome: Some("aborted".into()),
                expected_exit_code: Some(3),
                ..Default::default()
            }
            .into(),
            TxnBegin::default().into(),
            TxnCommit {
                txn: 1,
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 1,
                expected_error: Some("transaction not found".into()),
                expected_commit_outcome: Some("indeterminate".into()),
                expected_exit_code: Some(4),
                ..Default::default()
            }
            .into(),
        ])
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.commit_outcome.no_open_txn")]
#[mantra_macros::req_test("defra.txn.commit_outcome.cli_exit")]
fn a_commit_of_a_malformed_id_has_no_outcome() {
    TestCase {
        client_types: Some(HTTP_CLIENTS),
        ..TestCase::from_actions(vec![TxnCommit {
            raw_id: Some("concurrent".into()),
            expected_error: Some("invalid transaction id".into()),
            expected_exit_code: Some(1),
            ..Default::default()
        }
        .into()])
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.commit_outcome.statement_atomicity")]
fn a_failed_statement_in_an_explicit_transaction_leaves_nothing_and_keeps_earlier_ones() {
    TestCase {
        client_types: Some(RUST_CLIENTS),
        ..TestCase::from_actions(vec![
            add_collection(),
            TxnBegin::default().into(),
            add_in(0, "Shahzad"),
            AddDoc {
                txn: Some(0),
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                expected_error: Some("a document with the given ID already exists".into()),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            users(json!([{ "name": "Shahzad" }])),
        ])
    }
    .execute();
}
