//! A query's one-to-one join reads the related documents as a query read: it
//! decides nothing, so another writer landing a book in the range the join
//! scanned does not fail the explicit transaction's commit. A writer linking
//! the same publisher still loses, through its own uniqueness check.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request, TxnBegin, TxnCommit};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

const SDL: &str = r#"
    type Book {
        name: String
        publisher: Publisher @primary
    }

    type Publisher {
        name: String
        published: Book
    }
"#;

const CONFLICT: &str = "transaction conflict. Please retry";

fn publisher(name: &str, capture: &str) -> Actions {
    AddDoc {
        collection: "Publisher".into(),
        doc_map: Some(json!({ "name": name })),
        common: Common {
            capture: captures([(capture, "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn add_book(txn: Option<usize>, name: &str, publisher: &str) -> Actions {
    Request {
        txn,
        request: Text::concat([
            Text::from(format!(
                r#"mutation {{ add_Book(input: {{name: "{name}", _publisherID: ""#
            )),
            Text::reference(publisher),
            Text::from(r#""}) { _docID } }"#),
        ]),
        ..Default::default()
    }
    .into()
}

/// The transaction adds its book, then runs the one-to-one join over every
/// book, and an autocommit writer adds a book for `other` before the
/// transaction commits.
fn link_join_then_commit(mine: &str, other: &str, error: Option<&str>) -> Vec<Actions> {
    vec![
        AddCollection {
            sdl: SDL.into(),
            ..Default::default()
        }
        .into(),
        publisher("Website", "website"),
        publisher("Online", "online"),
        TxnBegin::default().into(),
        add_book(Some(0), "Mine", mine),
        Request {
            txn: Some(0),
            request: "query { Book { name publisher { name } } }".into(),
            expected_data: json!({
                "Book": [{ "name": "Mine", "publisher": { "name": "Website" } }],
            }),
            ..Default::default()
        }
        .into(),
        add_book(None, "Theirs", other),
        TxnCommit {
            txn: 0,
            expected_error: error.map(str::to_owned),
            ..Default::default()
        }
        .into(),
    ]
}

fn books(expected: serde_json::Value) -> Actions {
    Request {
        request: "query { Book(order: {name: ASC}) { name publisher { name } } }".into(),
        expected_data: json!({ "Book": expected }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.txn.relation_join_read")]
fn a_one_to_one_join_read_does_not_fail_the_commit() {
    let mut actions = link_join_then_commit("website", "online", None);
    actions.push(books(json!([
        { "name": "Mine", "publisher": { "name": "Website" } },
        { "name": "Theirs", "publisher": { "name": "Online" } },
    ])));
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.relation_join_read")]
fn a_writer_of_the_same_link_still_loses_the_commit() {
    let mut actions = link_join_then_commit("website", "website", Some(CONFLICT));
    actions.push(books(json!([
        { "name": "Theirs", "publisher": { "name": "Website" } },
    ])));
    TestCase::from_actions(actions).execute();
}
