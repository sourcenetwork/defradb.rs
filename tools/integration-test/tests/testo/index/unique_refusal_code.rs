//! A mutation a unique index refuses carries the `UNIQUE_CONSTRAINT_VIOLATION`
//! code beside its unchanged message (`defra.index.unique.refusal-code`,
//! `defra.txn.isolation.unique-autocommit`). Go and the CLI report no code,
//! so the cases that check it run under the in-process and Rust HTTP clients;
//! the message alone is checked under every client.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

const VIOLATION: &str = "can not index a doc's field(s) that violates unique index";
const CODE: &str = "UNIQUE_CONSTRAINT_VIOLATION";
const CODE_CLIENTS: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp];

/// The DocID of `{"name":"alice","age":2}`, the refused writer.
const ALICE_2: &str = "bae-0dd8fc64-0dc2-54f5-b156-52e560dff6f3";
/// The DocID of `{"name":"bob","age":3}`, the refused update's writer.
const BOB_3: &str = "bae-f2895d11-b901-58b7-843c-61b2ecc65816";

fn collection() -> Actions {
    AddCollection {
        sdl: "type User { name: String @index(unique: true) age: Int }".into(),
        ..Default::default()
    }
    .into()
}

fn add(name: &str, age: i64, capture: Option<&str>) -> Actions {
    Request {
        request: format!(
            r#"mutation {{ add_User(input: {{name: "{name}", age: {age}}}) {{ _docID }} }}"#
        )
        .into(),
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

fn refused_add(name: &str, age: i64, error: String, code: Option<&str>) -> Actions {
    Request {
        request: format!(
            r#"mutation {{ add_User(input: {{name: "{name}", age: {age}}}) {{ _docID }} }}"#
        )
        .into(),
        expected_error: Some(error),
        expected_error_code: code.map(str::to_owned),
        ..Default::default()
    }
    .into()
}

fn only_alice() -> Actions {
    Request {
        request: "query { User { name age } }".into(),
        expected_data: json!({ "User": [{ "name": "alice", "age": 1 }] }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.index.unique.refusal-code")]
fn a_create_refused_by_a_unique_index_carries_its_code() {
    TestCase {
        client_types: Some(CODE_CLIENTS),
        ..TestCase::from_actions(vec![
            collection(),
            add("alice", 1, None),
            refused_add(
                "alice",
                2,
                format!("{VIOLATION}. DocID: {ALICE_2}, name: alice"),
                Some(CODE),
            ),
        ])
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.unique.refusal-code")]
fn an_update_refused_by_a_unique_index_carries_its_code() {
    TestCase {
        client_types: Some(CODE_CLIENTS),
        ..TestCase::from_actions(vec![
            collection(),
            add("alice", 1, None),
            add("bob", 3, Some("bob")),
            Request {
                request: Text::concat([
                    Text::from(r#"mutation { update_User(docID: [""#),
                    Text::reference("bob"),
                    Text::from(r#""], input: {name: "alice"}) { _docID } }"#),
                ]),
                expected_error: Some(format!(
                    "failed to update index: {VIOLATION}. DocID: {BOB_3}, name: alice. IndexName: User_name_ASC"
                )),
                expected_error_code: Some(CODE.into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { User(order: {age: ASC}) { name age } }".into(),
                expected_data: json!({ "User": [
                    { "name": "alice", "age": 1 },
                    { "name": "bob", "age": 3 },
                ] }),
                ..Default::default()
            }
            .into(),
        ])
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.isolation.unique-autocommit")]
fn a_second_autocommit_create_of_a_value_carries_its_code() {
    TestCase {
        client_types: Some(CODE_CLIENTS),
        ..TestCase::from_actions(vec![
            collection(),
            add("alice", 1, None),
            refused_add(
                "alice",
                2,
                format!("{VIOLATION}. DocID: {ALICE_2}, name: alice"),
                Some(CODE),
            ),
            only_alice(),
        ])
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.isolation.unique-autocommit")]
fn a_second_autocommit_create_of_a_value_is_refused_and_stores_nothing() {
    TestCase::from_actions(vec![
        collection(),
        add("alice", 1, None),
        refused_add(
            "alice",
            2,
            format!("{VIOLATION}. DocID: {ALICE_2}, name: alice"),
            None,
        ),
        only_alice(),
    ])
    .execute();
}

/// The document API (`AddDoc`: the node's `add_docs` in process, `POST
/// /collections/User` under the Rust HTTP client) refuses the second alice
/// as a unique violation, its message unchanged, and stores nothing.
#[test]
#[mantra_macros::req_test("defra.index.unique.refusal-code")]
fn a_document_add_refused_by_a_unique_index_carries_its_code() {
    TestCase {
        client_types: Some(CODE_CLIENTS),
        ..TestCase::from_actions(vec![
            collection(),
            add("alice", 1, None),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "alice", "age": 2 })),
                expected_error: Some(format!("{VIOLATION}. DocID: {ALICE_2}, name: alice")),
                expected_error_code: Some(CODE.into()),
                ..Default::default()
            }
            .into(),
            only_alice(),
        ])
    }
    .execute();
}
