//! A query filter answered through a unique index reads the index entry as a
//! query read: it decides nothing, so another writer adding the value the
//! filter asked for does not fail the explicit transaction's commit. The
//! unique entry the transaction itself writes is still validated.

use integration_test::actions::{Actions, AddCollection, Request, TxnBegin, TxnCommit};
use integration_test::test_case::TestCase;
use serde_json::json;

const UNIQUE: &str = "type User { name: String @index(unique: true) age: Int }";
const CONFLICT: &str = "transaction conflict. Please retry";

fn collection() -> Actions {
    AddCollection {
        sdl: UNIQUE.into(),
        ..Default::default()
    }
    .into()
}

fn finds_no_alice(filter: &str) -> Actions {
    Request {
        request: format!("query {{ User(filter: {{name: {filter}}}) {{ name }} }}").into(),
        txn: Some(0),
        expected_data: json!({ "User": [] }),
        ..Default::default()
    }
    .into()
}

fn add(txn: Option<usize>, input: &str) -> Actions {
    Request {
        request: format!("mutation {{ add_User(input: {{{input}}}) {{ _docID }} }}").into(),
        txn,
        ..Default::default()
    }
    .into()
}

fn commit(error: Option<&str>) -> Actions {
    TxnCommit {
        txn: 0,
        expected_error: error.map(str::to_owned),
        ..Default::default()
    }
    .into()
}

fn users(expected: serde_json::Value) -> Actions {
    Request {
        request: "query { User(order: {age: ASC}) { name age } }".into(),
        expected_data: json!({ "User": expected }),
        ..Default::default()
    }
    .into()
}

/// The transaction filters for alice, writes `input`, and commits after an
/// autocommit add of alice.
fn filter_then_write(filter: &str, input: &str, error: Option<&str>) -> Vec<Actions> {
    vec![
        collection(),
        TxnBegin::default().into(),
        finds_no_alice(filter),
        add(Some(0), input),
        add(None, r#"name: "alice", age: 1"#),
        commit(error),
    ]
}

/// Go validates the filter read and fails the commit; this test is in
/// `.github/testo/divergences.txt`.
#[test]
#[mantra_macros::req_test("defra.txn.docs.unique-filter-read")]
fn a_unique_eq_filter_read_does_not_fail_the_commit() {
    let mut actions = filter_then_write(r#"{_eq: "alice"}"#, r#"name: "bob", age: 3"#, None);
    actions.push(users(json!([
        { "name": "alice", "age": 1 },
        { "name": "bob", "age": 3 },
    ])));
    TestCase::from_actions(actions).execute();
}

/// Go validates the filter read and fails the commit; this test is in
/// `.github/testo/divergences.txt`.
#[test]
#[mantra_macros::req_test("defra.txn.docs.unique-filter-read")]
fn a_unique_in_filter_read_does_not_fail_the_commit() {
    let mut actions = filter_then_write(r#"{_in: ["alice"]}"#, r#"name: "bob", age: 3"#, None);
    actions.push(users(json!([
        { "name": "alice", "age": 1 },
        { "name": "bob", "age": 3 },
    ])));
    TestCase::from_actions(actions).execute();
}

/// Go validates the filter read and fails the commit; this test is in
/// `.github/testo/divergences.txt`.
#[test]
#[mantra_macros::req_test("defra.txn.docs.unique-filter-read")]
fn a_nil_writers_unique_filter_read_does_not_fail_the_commit() {
    let mut actions = filter_then_write(r#"{_eq: "alice"}"#, "age: 3", None);
    actions.push(users(json!([
        { "name": "alice", "age": 1 },
        { "name": null, "age": 3 },
    ])));
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.docs.unique-filter-read")]
fn a_writer_of_the_filtered_value_still_loses_the_commit() {
    let mut actions = filter_then_write(
        r#"{_eq: "alice"}"#,
        r#"name: "alice", age: 2"#,
        Some(CONFLICT),
    );
    actions.push(users(json!([{ "name": "alice", "age": 1 }])));
    TestCase::from_actions(actions).execute();
}
