//! A filtered delete follows a relation in its filter the same way a query
//! does (#2099, `defra.request.mutation_relation_filter`).

use integration_test::actions::{Actions, AddCollection, AddDoc, Request, TxnBegin, TxnCommit};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

fn person(name: &str, capture: &str) -> Actions {
    AddDoc {
        collection: "Person".into(),
        doc_map: Some(json!({ "name": name })),
        common: Common {
            capture: captures([(capture, "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn book(title: &str, genre: &str, author: &str) -> Actions {
    AddDoc {
        collection: "Book".into(),
        doc_map: Some(json!({ "title": title, "genre": genre, "_authorID": m::r(author) })),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.request.mutation_relation_filter")]
fn a_delete_filtered_through_a_relation_deletes_what_the_query_finds() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: r#"
                type Book { title: String genre: String author: Person }
                type Person { name: String authoredBooks: [Book] }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
        person("George Orwell", "orwell"),
        person("William Golding", "golding"),
        person("Jane Austen", "austen"),
        book("1984", "Dystopia", "orwell"),
        book("Lord of the Flies", "Dystopia", "golding"),
        book("Emma", "Romance", "austen"),
        Request {
            request: r#"mutation {
                delete_Person(filter: {authoredBooks: {genre: {_eq: "Dystopia"}}}) { name }
            }"#
            .into(),
            expected_data: json!({ "delete_Person": [
                { "name": "George Orwell" }, { "name": "William Golding" }
            ] }),
            common: Common {
                unordered_documents: vec!["/data/delete_Person".into()],
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Person { name } }".into(),
            expected_data: json!({ "Person": [{ "name": "Jane Austen" }] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.request.mutation_relation_filter")]
fn an_update_filtered_through_a_relation_updates_what_the_query_finds() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: r#"
                type Book { title: String genre: String author: Person }
                type Person { name: String flagged: Boolean authoredBooks: [Book] }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
        person("George Orwell", "orwell"),
        person("Jane Austen", "austen"),
        book("1984", "Dystopia", "orwell"),
        book("Emma", "Romance", "austen"),
        Request {
            request: r#"mutation {
                update_Person(filter: {authoredBooks: {genre: {_eq: "Dystopia"}}}, input: {flagged: true}) { name }
            }"#
            .into(),
            expected_data: json!({ "update_Person": [{ "name": "George Orwell" }] }),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Person(filter: {flagged: {_eq: true}}) { name } }".into(),
            expected_data: json!({ "Person": [{ "name": "George Orwell" }] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.request.mutation_relation_filter")]
fn a_concurrent_change_to_a_related_document_aborts_the_delete() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: r#"
                type Book { title: String genre: String author: Person }
                type Person { name: String authoredBooks: [Book] }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
        person("George Orwell", "orwell"),
        book("1984", "Dystopia", "orwell"),
        TxnBegin::default().into(),
        TxnBegin::default().into(),
        Request {
            request: r#"mutation {
                delete_Person(filter: {authoredBooks: {genre: {_eq: "Dystopia"}}}) { name }
            }"#
            .into(),
            expected_data: json!({ "delete_Person": [{ "name": "George Orwell" }] }),
            txn: Some(0),
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"mutation {
                update_Book(filter: {title: {_eq: "1984"}}, input: {genre: "Satire"}) { title }
            }"#
            .into(),
            expected_data: json!({ "update_Book": [{ "title": "1984" }] }),
            txn: Some(1),
            ..Default::default()
        }
        .into(),
        TxnCommit {
            txn: 1,
            ..Default::default()
        }
        .into(),
        TxnCommit {
            txn: 0,
            expected_error: Some("transaction conflict. Please retry".into()),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Person { name } }".into(),
            expected_data: json!({ "Person": [{ "name": "George Orwell" }] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
