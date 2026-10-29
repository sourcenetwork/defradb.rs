//! Port of Go's explicit-transaction tests: a transaction travels in the
//! request context, and every client type gives the same answers.

use integration_test::actions::{Actions, AddCollection, Request, TxnBegin, TxnCommit, TxnDiscard};
use integration_test::test_case::TestCase;
use serde_json::json;

const SDL: &str = "type Users { name: String }";
const CREATE: &str = r#"mutation { add_Users(input: {name: "Alice"}) { name } }"#;
const READ: &str = "query { Users { name } }";

fn schema() -> Actions {
    AddCollection {
        sdl: SDL.into(),
        ..Default::default()
    }
    .into()
}

fn create_in(txn: usize) -> Actions {
    Request {
        request: CREATE.into(),
        expected_data: json!({ "add_Users": [{ "name": "Alice" }] }),
        txn: Some(txn),
        ..Default::default()
    }
    .into()
}

fn read(txn: Option<usize>, names: &[&str]) -> Actions {
    let rows: Vec<_> = names.iter().map(|name| json!({ "name": name })).collect();
    Request {
        request: READ.into(),
        expected_data: json!({ "Users": rows }),
        txn,
        ..Default::default()
    }
    .into()
}

fn begin() -> Actions {
    TxnBegin::default().into()
}

#[test]
fn write_is_visible_inside_its_txn_and_not_outside_until_commit() {
    TestCase::from_actions(vec![
        schema(),
        begin(),
        create_in(0),
        read(Some(0), &["Alice"]),
        read(None, &[]),
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
        read(None, &["Alice"]),
    ])
    .execute();
}

#[test]
fn discard_leaves_nothing_behind() {
    TestCase::from_actions(vec![
        schema(),
        begin(),
        create_in(0),
        TxnDiscard {
            txn: 0,
            ..Default::default()
        }
        .into(),
        read(None, &[]),
    ])
    .execute();
}

#[test]
fn a_finished_txn_is_not_found() {
    let not_found = Some("transaction not found".to_owned());
    TestCase::from_actions(vec![
        schema(),
        begin(),
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
        TxnCommit {
            txn: 0,
            expected_error: not_found.clone(),
        }
        .into(),
        TxnDiscard {
            txn: 0,
            expected_error: not_found.clone(),
        }
        .into(),
        Request {
            request: READ.into(),
            txn: Some(0),
            expected_error: not_found,
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
fn two_txns_writing_the_same_document_conflict() {
    let update = |name: &str, txn: usize| -> Actions {
        Request {
            request: format!(
                r#"mutation {{ update_Users(filter: {{name: {{_eq: "Alice"}}}}, input: {{name: "{name}"}}) {{ name }} }}"#
            ),
            expected_data: json!({ "update_Users": [{ "name": name }] }),
            txn: Some(txn),
            ..Default::default()
        }
        .into()
    };
    TestCase::from_actions(vec![
        schema(),
        Request {
            request: CREATE.into(),
            expected_data: json!({ "add_Users": [{ "name": "Alice" }] }),
            ..Default::default()
        }
        .into(),
        begin(),
        begin(),
        update("Bob", 0),
        update("Carol", 1),
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
        TxnCommit {
            txn: 1,
            expected_error: Some("transaction conflict. Please retry".into()),
        }
        .into(),
        read(None, &["Bob"]),
    ])
    .execute();
}

#[test]
fn read_only_txn_rejects_the_first_write() {
    TestCase::from_actions(vec![
        schema(),
        TxnBegin {
            read_only: true,
            ..Default::default()
        }
        .into(),
        Request {
            request: CREATE.into(),
            txn: Some(0),
            expected_error: Some(
                "No sets or deletes are allowed in a read-only transaction".into(),
            ),
            ..Default::default()
        }
        .into(),
        read(Some(0), &[]),
    ])
    .execute();
}

#[test]
fn collections_added_in_a_txn_appear_on_commit() {
    TestCase::from_actions(vec![
        begin(),
        AddCollection {
            sdl: SDL.into(),
            txn: Some(0),
            ..Default::default()
        }
        .into(),
        create_in(0),
        Request {
            request: READ.into(),
            expected_error: Some(r#"Cannot query field "Users""#.into()),
            ..Default::default()
        }
        .into(),
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
        read(None, &["Alice"]),
    ])
    .execute();
}
