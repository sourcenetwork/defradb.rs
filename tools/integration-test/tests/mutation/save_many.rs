//! Save many documents in one transaction (`defra.save_many`). It is a core
//! API with no HTTP or CLI route, so these run against Rust nodes only.

use integration_test::actions::{
    Actions, AddCollection, AddDoc, DeleteDoc, Request, SaveDocs, TxnBegin, TxnCommit, TxnDiscard,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn users() -> Actions {
    AddCollection {
        sdl: "type User { name: String age: Int }".into(),
        ..Default::default()
    }
    .into()
}

fn alice() -> Actions {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": "Alice", "age": 40 })),
        common: Common {
            capture: captures([("alice", "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn save(docs: Value) -> SaveDocs {
    SaveDocs {
        collection: "User".into(),
        docs: serde_json::from_value(docs).unwrap(),
        ..Default::default()
    }
}

fn users_are(expected: Value) -> Actions {
    Request {
        request: "query { User(order: {name: ASC}) { name age } }".into(),
        expected_data: json!({ "User": expected }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.save_many.create_or_update")]
fn save_many_updates_a_stored_document_and_adds_a_new_one() {
    TestCase::from_actions(vec![
        users(),
        alice(),
        save(json!([
            { "_docID": { "$ref": "alice" }, "age": 41 },
            { "name": "Bob", "age": 50 },
        ]))
        .into(),
        users_are(json!([
            { "name": "Alice", "age": 41 },
            { "name": "Bob", "age": 50 },
        ])),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.save_many.atomic")]
fn one_document_that_fails_saves_none() {
    TestCase::from_actions(vec![
        users(),
        SaveDocs {
            expected_error: Some("nickname".into()),
            ..save(json!([
                { "name": "Bob" },
                { "name": "Carol", "nickname": "C" },
            ]))
        }
        .into(),
        users_are(json!([])),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.save_many.deleted")]
fn a_deleted_document_is_not_saved_again() {
    TestCase::from_actions(vec![
        users(),
        alice(),
        DeleteDoc {
            collection: "User".into(),
            doc_id: Text::reference("alice"),
            ..Default::default()
        }
        .into(),
        SaveDocs {
            expected_error: Some("deleted".into()),
            ..save(json!([
                { "name": "Bob" },
                { "_docID": { "$ref": "alice" }, "age": 41 },
            ]))
        }
        .into(),
        users_are(json!([])),
    ])
    .execute();
}

fn in_a_transaction(end: Actions, expected: Value) -> Vec<Actions> {
    vec![
        users(),
        TxnBegin::default().into(),
        SaveDocs {
            txn: Some(0),
            ..save(json!([{ "name": "Bob", "age": 50 }]))
        }
        .into(),
        users_are(json!([])),
        end,
        users_are(expected),
    ]
}

#[test]
#[mantra_macros::req_test("defra.save_many.transaction")]
fn a_batch_in_a_transaction_lands_on_commit() {
    TestCase::from_actions(in_a_transaction(
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
        json!([{ "name": "Bob", "age": 50 }]),
    ))
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.save_many.transaction")]
fn a_discarded_batch_stores_nothing() {
    TestCase::from_actions(in_a_transaction(
        TxnDiscard {
            txn: 0,
            ..Default::default()
        }
        .into(),
        json!([]),
    ))
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.save_many.transaction")]
fn an_empty_batch_stores_nothing() {
    TestCase::from_actions(vec![
        users(),
        SaveDocs {
            expected_data: json!({ "docIDs": [] }),
            ..save(json!([]))
        }
        .into(),
        users_are(json!([])),
    ])
    .execute();
}
