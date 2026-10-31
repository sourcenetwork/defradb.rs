//! Purge by DocID (`defra.purge`). It is a core API with no HTTP or CLI
//! route, so these run against Rust nodes only. What the purge leaves in the
//! block store is checked in `crates/db/tests/collection/purge.rs`.

use integration_test::actions::{
    Actions, AddCollection, AddDoc, PurgeDocs, Request, TxnBegin, TxnCommit, TxnDiscard,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const USERS: &str = "type User { name: String age: Int }";

fn collection(sdl: &str) -> Actions {
    AddCollection {
        sdl: sdl.into(),
        ..Default::default()
    }
    .into()
}

fn add(name: &str, age: i64, capture: &str) -> Actions {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": name, "age": age })),
        common: Common {
            capture: captures([(capture, "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn purge(doc_ids: &[&str]) -> PurgeDocs {
    PurgeDocs {
        collection: "User".into(),
        doc_ids: doc_ids.iter().map(|id| Text::reference(id)).collect(),
        ..Default::default()
    }
}

fn names(expected: Value) -> Actions {
    Request {
        request: "query { User(order: {name: ASC}) { name } }".into(),
        expected_data: json!({ "User": expected }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.purge.removes_document")]
fn purge_removes_the_document() {
    TestCase::from_actions(vec![
        collection(USERS),
        add("Alice", 40, "alice"),
        add("Bob", 50, "bob"),
        purge(&["alice"]).into(),
        names(json!([{ "name": "Bob" }])),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.purge.removes_document")]
fn purge_removes_the_unique_index_entry() {
    TestCase::from_actions(vec![
        collection("type User { name: String @index(unique: true) age: Int }"),
        add("alice", 40, "alice"),
        purge(&["alice"]).into(),
        add("alice", 41, "again"),
        names(json!([{ "name": "alice" }])),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.purge.mappings")]
fn purged_doc_id_no_longer_resolves() {
    TestCase::from_actions(vec![
        collection(USERS),
        add("Alice", 40, "alice"),
        purge(&["alice"]).into(),
        Request {
            request: Text::Concat {
                parts: vec![
                    r#"query { User(docID: ""#.into(),
                    Text::reference("alice"),
                    r#"") { name } }"#.into(),
                ],
            },
            expected_data: json!({ "User": [] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.purge.history")]
fn a_purged_document_can_be_added_again() {
    TestCase::from_actions(vec![
        collection(USERS),
        add("Alice", 40, "alice"),
        add("Alice", 7, "carol"),
        purge(&["alice"]).into(),
        add("Alice", 40, "again"),
        names(json!([{ "name": "Alice" }, { "name": "Alice" }])),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.purge.count")]
fn purge_counts_each_stored_document_once() {
    TestCase::from_actions(vec![
        collection(USERS),
        add("Alice", 40, "alice"),
        add("Bob", 50, "bob"),
        PurgeDocs {
            expected_count: Some(2),
            ..purge(&["alice", "bob", "alice"])
        }
        .into(),
        PurgeDocs {
            expected_count: Some(0),
            ..purge(&["alice"])
        }
        .into(),
        names(json!([])),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.purge.count")]
fn a_discarded_purge_removes_nothing() {
    TestCase::from_actions(vec![
        collection(USERS),
        add("Alice", 40, "alice"),
        TxnBegin::default().into(),
        PurgeDocs {
            txn: Some(0),
            ..purge(&["alice"])
        }
        .into(),
        TxnDiscard {
            txn: 0,
            ..Default::default()
        }
        .into(),
        names(json!([{ "name": "Alice" }])),
    ])
    .execute();
}

fn purge_in_a_transaction(read_before_commit: bool) -> Vec<Actions> {
    let mut actions = vec![
        collection(USERS),
        add("Alice", 40, "alice"),
        add("Bob", 50, "bob"),
        TxnBegin::default().into(),
        PurgeDocs {
            txn: Some(0),
            expected_count: Some(1),
            ..purge(&["alice"])
        }
        .into(),
    ];
    if read_before_commit {
        actions.push(names(json!([{ "name": "Alice" }, { "name": "Bob" }])));
    }
    actions.push(
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
    );
    actions.push(names(json!([{ "name": "Bob" }])));
    actions
}

#[test]
#[mantra_macros::req_test("defra.purge.count")]
fn a_purge_in_a_transaction_lands_on_commit() {
    TestCase::from_actions(purge_in_a_transaction(false)).execute();
}

/// A read outside the transaction still sees the document until it commits.
#[test]
#[mantra_macros::req_test("defra.purge.count")]
fn a_purge_in_a_transaction_is_invisible_until_commit() {
    TestCase::from_actions(purge_in_a_transaction(true)).execute();
}
