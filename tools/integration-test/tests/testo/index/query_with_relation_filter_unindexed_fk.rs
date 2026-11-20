//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.
//!
//! A relation filter on an indexed child field whose parent FK has no index
//! (#3087): `Person.name` is indexed and `Book.author` is not.

use integration_test::actions::{Actions, AddCollection, AddDoc, DeleteDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};
use testo::action::ActionInstance;

type Action = ActionInstance<Actions>;

const SDL: &str = r#"
    type Person {
        name: String @index
        books: [Book]
    }
    type Book {
        title: String @index
        author: Person
        publisher: Publisher
    }
    type Publisher {
        name: String
        books: [Book]
    }
"#;

fn schema() -> Action {
    AddCollection {
        sdl: SDL.into(),
        ..Default::default()
    }
    .into()
}

/// A document of `collection`, captured as `name`.
fn doc(collection: &str, name: &str, fields: Value) -> Action {
    AddDoc {
        collection: collection.into(),
        doc_map: Some(fields),
        common: Common {
            capture: captures([(name, "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn request(query: &str, expected: Value) -> Action {
    Request {
        request: query.into(),
        expected_data: expected,
        ..Default::default()
    }
    .into()
}

/// Ann and Bob, and their Books a (Bob), b (Ann), c (Bob) and d (Ann).
fn authors_and_books() -> Vec<Action> {
    vec![
        schema(),
        doc("Person", "ann", json!({ "name": "Ann" })),
        doc("Person", "bob", json!({ "name": "Bob" })),
        doc("Book", "a", json!({ "title": "a", "author": m::r("bob") })),
        doc("Book", "b", json!({ "title": "b", "author": m::r("ann") })),
        doc("Book", "c", json!({ "title": "c", "author": m::r("bob") })),
        doc("Book", "d", json!({ "title": "d", "author": m::r("ann") })),
    ]
}

/// The order a parent index provides is the order of the result.
#[test]
fn relation_filter_keeps_the_order_a_parent_index_provides() {
    let mut actions = authors_and_books();
    actions.push(request(
        r#"query {
            Book(filter: {author: {name: {_in: ["Ann", "Bob"]}}}, order: {title: ASC}) {
                title
            }
        }"#,
        json!({ "Book": [
            { "title": "a" }, { "title": "b" }, { "title": "c" }, { "title": "d" }
        ] }),
    ));
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}

/// A limit applies to the filtered, ordered result.
#[test]
fn relation_filter_with_limit_returns_the_first_matching_rows() {
    let mut actions = authors_and_books();
    actions.push(request(
        r#"query {
            Book(filter: {author: {name: {_eq: "Ann"}}}, order: {title: DESC}, limit: 1) {
                title
            }
        }"#,
        json!({ "Book": [{ "title": "d" }] }),
    ));
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}

/// A deleted Book is returned when deleted documents are asked for.
#[test]
fn relation_filter_with_show_deleted_returns_the_deleted_parent() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            schema(),
            doc("Person", "x", json!({ "name": "X" })),
            doc(
                "Book",
                "gone",
                json!({ "title": "gone", "author": m::r("x") }),
            ),
            DeleteDoc {
                collection: "Book".into(),
                doc_id: Text::reference("gone"),
                ..Default::default()
            }
            .into(),
            request(
                r#"query {
                    Book(showDeleted: true, filter: {author: {name: {_eq: "X"}}}) {
                        title
                        _deleted
                    }
                }"#,
                json!({ "Book": [{ "title": "gone", "_deleted": true }] }),
            ),
        ],
        ..Default::default()
    }
    .execute();
}

/// A Book with no author never matches a filter on its author.
#[test]
fn relation_filter_skips_a_book_without_an_author() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            schema(),
            doc("Person", "ann", json!({ "name": "Ann" })),
            doc("Book", "orphan", json!({ "title": "orphan" })),
            doc("Book", "a", json!({ "title": "a", "author": m::r("ann") })),
            request(
                r#"query {
                    Book(filter: {author: {name: {_eq: "Ann"}}}) {
                        title
                    }
                }"#,
                json!({ "Book": [{ "title": "a" }] }),
            ),
        ],
        ..Default::default()
    }
    .execute();
}

/// Another relation selected on the Book is still rendered.
#[test]
fn relation_filter_keeps_another_selected_relation() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            schema(),
            doc("Person", "ann", json!({ "name": "Ann" })),
            doc("Person", "bob", json!({ "name": "Bob" })),
            doc("Publisher", "penguin", json!({ "name": "Penguin" })),
            doc(
                "Book",
                "a",
                json!({ "title": "a", "author": m::r("ann"), "publisher": m::r("penguin") }),
            ),
            doc("Book", "b", json!({ "title": "b", "author": m::r("bob") })),
            request(
                r#"query {
                    Book(filter: {author: {name: {_eq: "Ann"}}}) {
                        title
                        publisher { name }
                    }
                }"#,
                json!({ "Book": [{ "title": "a", "publisher": { "name": "Penguin" } }] }),
            ),
        ],
        ..Default::default()
    }
    .execute();
}
