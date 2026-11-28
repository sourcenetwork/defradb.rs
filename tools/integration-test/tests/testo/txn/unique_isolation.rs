//! What a unique index guarantees to writers racing on one node
//! (`defra.txn.isolation`). Two explicit transactions interleave
//! deterministically: each reads the value as absent, each adds it, and the
//! second commit loses. The documents differ in `age`, so their DocIDs differ
//! and the race is decided on the unique entry, not on the DocID mapping.

use integration_test::actions::{Actions, AddCollection, Request, TxnBegin, TxnCommit};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

const UNIQUE: &str = "type User { name: String @index(unique: true) age: Int }";
const PLAIN: &str = "type User { name: String age: Int }";
const VIOLATION: &str = "can not index a doc's field(s) that violates unique index";
const CONFLICT: &str = "transaction conflict. Please retry";
const CODE: &str = "UNIQUE_CONSTRAINT_VIOLATION";

/// The DocID of `{"name":"alice","age":2}`, the loser's document.
const ALICE_2: &str = "bae-0dd8fc64-0dc2-54f5-b156-52e560dff6f3";

fn collection(sdl: &str) -> Actions {
    AddCollection {
        sdl: sdl.into(),
        ..Default::default()
    }
    .into()
}

fn finds_no_alice(txn: usize) -> Actions {
    Request {
        request: r#"query { User(filter: {name: {_eq: "alice"}}) { name } }"#.into(),
        txn: Some(txn),
        expected_data: json!({ "User": [] }),
        ..Default::default()
    }
    .into()
}

fn add(txn: Option<usize>, input: &str, capture: Option<&str>) -> Actions {
    Request {
        request: format!("mutation {{ add_User(input: {{{input}}}) {{ _docID }} }}").into(),
        txn,
        common: Common {
            capture: capture.map_or_else(Default::default, |c| {
                captures([(c, "/data/add_User/0/_docID")])
            }),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn commit(txn: usize, error: Option<&str>) -> Actions {
    TxnCommit {
        txn,
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

/// Two explicit transactions add alice with different ages after each read
/// her as absent; transaction 0 commits first.
fn interleaved(sdl: &str, loser_error: Option<&str>) -> Vec<Actions> {
    vec![
        collection(sdl),
        TxnBegin::default().into(),
        TxnBegin::default().into(),
        finds_no_alice(0),
        finds_no_alice(1),
        add(Some(0), r#"name: "alice", age: 1"#, None),
        add(Some(1), r#"name: "alice", age: 2"#, Some("loser")),
        commit(0, None),
        commit(1, loser_error),
    ]
}

/// The unique race, its aftermath, and the loser's create run again, with
/// the refusal's code checked when `code` is set.
fn unique_race(code: Option<&str>) -> Vec<Actions> {
    let mut actions = interleaved(UNIQUE, Some(CONFLICT));
    actions.extend([
        users(json!([{ "name": "alice", "age": 1 }])),
        Request {
            request: Text::concat([
                Text::from(r#"query { User(docID: ""#),
                Text::reference("loser"),
                Text::from(r#"") { name } }"#),
            ]),
            expected_data: json!({ "User": [] }),
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"mutation { add_User(input: {name: "alice", age: 2}) { _docID } }"#.into(),
            expected_error: Some(format!("{VIOLATION}. DocID: {ALICE_2}, name: alice")),
            expected_error_code: code.map(str::to_owned),
            ..Default::default()
        }
        .into(),
        users(json!([{ "name": "alice", "age": 1 }])),
    ]);
    actions
}

#[test]
#[mantra_macros::req_test("defra.txn.isolation.unique-explicit")]
fn the_second_commit_of_a_unique_value_is_a_conflict() {
    TestCase::from_actions(unique_race(None)).execute();
}

#[test]
#[mantra_macros::req_test(
    "defra.txn.isolation.unique-explicit",
    "defra.txn.isolation.unique-explicit-retry"
)]
fn the_losers_rerun_is_refused_with_the_unique_code() {
    TestCase {
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp]),
        ..TestCase::from_actions(unique_race(Some(CODE)))
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.isolation.unique-nil")]
fn nil_values_are_not_refused() {
    TestCase::from_actions(vec![
        collection(UNIQUE),
        TxnBegin::default().into(),
        TxnBegin::default().into(),
        add(Some(0), "age: 1", None),
        add(Some(1), "age: 2", None),
        commit(0, None),
        commit(1, None),
        add(None, "age: 3", None),
        Request {
            request: "query { User(filter: {name: {_eq: null}}, order: {age: ASC}) { age } }"
                .into(),
            expected_data: json!({ "User": [{ "age": 1 }, { "age": 2 }, { "age": 3 }] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

/// A filter for nil is a prefix scan of the nil entries, which carry their
/// DocID, so it decides nothing: a transaction that read nil as absent and
/// wrote nil commits after another writer of nil.
#[test]
#[mantra_macros::req_test("defra.txn.isolation.unique-nil")]
fn a_nil_writers_filter_for_nil_decides_nothing() {
    TestCase::from_actions(vec![
        collection(UNIQUE),
        TxnBegin::default().into(),
        Request {
            request: "query { User(filter: {name: {_eq: null}}) { age } }".into(),
            txn: Some(0),
            expected_data: json!({ "User": [] }),
            ..Default::default()
        }
        .into(),
        add(Some(0), "age: 1", None),
        add(None, "age: 2", None),
        commit(0, None),
        users(json!([{ "name": null, "age": 1 }, { "name": null, "age": 2 }])),
    ])
    .execute();
}

/// Go validates the filter scan and aborts the second commit; this test is in
/// `.github/testo/divergences.txt`.
#[test]
#[mantra_macros::req_test("defra.txn.isolation.no-index-not-refused")]
fn without_an_index_both_commit() {
    let mut actions = interleaved(PLAIN, None);
    actions.push(users(json!([
        { "name": "alice", "age": 1 },
        { "name": "alice", "age": 2 },
    ])));
    TestCase::from_actions(actions).execute();
}
