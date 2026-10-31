//! A transaction that updates one document does not conflict with a writer
//! that rewrote the next document (Go shinzo-develop #5137,
//! `go.shinzo.docid.no_neighbour_conflict`). Both orders are tried, since
//! which document is next depends on their DocIDs.

use integration_test::actions::{AddCollection, AddDoc, Request, TxnBegin, TxnCommit, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

fn update_beside_a_rewritten_neighbour(reader: &str, writer: &str) {
    let mut actions = vec![AddCollection {
        sdl: "type User { name: String  age: Int }".into(),
        ..Default::default()
    }
    .into()];
    for (name, age) in [("alice", 1), ("bob", 2)] {
        actions.push(
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": name, "age": age })),
                common: Common {
                    capture: captures([(name, "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        );
    }
    actions.extend([
        TxnBegin::default().into(),
        UpdateDoc {
            collection: "User".into(),
            doc_id: Text::reference(reader),
            doc_map: json!({ "age": 10 }),
            txn: Some(0),
            ..Default::default()
        }
        .into(),
        // Another writer rewrites every field of the other document, and
        // commits first.
        UpdateDoc {
            collection: "User".into(),
            doc_id: Text::reference(writer),
            doc_map: json!({ "name": format!("{writer}2"), "age": 99 }),
            ..Default::default()
        }
        .into(),
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { User(order: {age: ASC}) { name age } }".into(),
            expected_data: json!({ "User": [
                { "name": reader, "age": 10 },
                { "name": format!("{writer}2"), "age": 99 },
            ] }),
            ..Default::default()
        }
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("go.shinzo.docid.no_neighbour_conflict")]
fn updating_alice_survives_a_rewrite_of_bob() {
    update_beside_a_rewritten_neighbour("alice", "bob");
}

#[test]
#[mantra_macros::req_test("go.shinzo.docid.no_neighbour_conflict")]
fn updating_bob_survives_a_rewrite_of_alice() {
    update_beside_a_rewritten_neighbour("bob", "alice");
}
