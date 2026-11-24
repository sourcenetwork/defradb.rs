//! A relation filter through a parent FK index keeps other joined relations.
//! Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn doc(collection: &str, capture: &str, fields: Value) -> Actions {
    AddDoc {
        collection: collection.into(),
        doc_map: Some(fields),
        common: Common {
            capture: captures([(capture, "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// Alice wrote A1 (Penguin, owned by Acme) and A2 (Other); Bob wrote B1 (Penguin).
/// A2 has the review "dull" and B1 the review "good".
fn library() -> Vec<Actions> {
    vec![
        AddCollection {
            sdl: r#"
                type Author {
                    name: String @index
                    books: [Book]
                }
                type Owner {
                    name: String
                    publisher: Publisher
                }
                type Publisher {
                    name: String
                    owner: Owner @primary
                    books: [Book]
                }
                type Book {
                    title: String
                    author: Author @index
                    publisher: Publisher
                    reviews: [Review]
                }
                type Review {
                    body: String
                    book: Book
                }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
        doc("Author", "alice", json!({ "name": "Alice" })),
        doc("Author", "bob", json!({ "name": "Bob" })),
        doc("Owner", "acme", json!({ "name": "Acme" })),
        doc(
            "Publisher",
            "penguin",
            json!({ "name": "Penguin", "owner": m::r("acme") }),
        ),
        doc("Publisher", "other", json!({ "name": "Other" })),
        doc(
            "Book",
            "a1",
            json!({ "title": "A1", "author": m::r("alice"), "publisher": m::r("penguin") }),
        ),
        doc(
            "Book",
            "a2",
            json!({ "title": "A2", "author": m::r("alice"), "publisher": m::r("other") }),
        ),
        doc(
            "Book",
            "b1",
            json!({ "title": "B1", "author": m::r("bob"), "publisher": m::r("penguin") }),
        ),
        doc(
            "Review",
            "dull",
            json!({ "body": "dull", "book": m::r("a2") }),
        ),
        doc(
            "Review",
            "good",
            json!({ "body": "good", "book": m::r("b1") }),
        ),
    ]
}

fn expect(id: &'static str, request: &str, expected_data: Value) {
    let mut actions = library();
    actions.push(
        Request {
            request: request.into(),
            expected_data,
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        id,
        actions: actions.into_iter().map(Into::into).collect(),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.relation.filter_keeps_joined_parent")]
fn relation_filter_through_an_indexed_fk_keeps_another_joined_relation() {
    expect(
        integration_test::scenario_id!(),
        r#"
            query {
                Book(filter: {author: {name: {_eq: "Bob"}}}) {
                    title
                    publisher { name }
                }
            }
        "#,
        json!({
            "Book": [{ "title": "B1", "publisher": { "name": "Penguin" } }],
        }),
    );
}

#[test]
#[mantra_macros::req_test("defra.index.relation.filter_keeps_joined_parent")]
fn relation_filter_through_an_indexed_fk_keeps_a_nested_joined_relation() {
    expect(
        integration_test::scenario_id!(),
        r#"
            query {
                Book(filter: {author: {name: {_eq: "Alice"}}, title: {_eq: "A1"}}) {
                    title
                    publisher { name owner { name } }
                }
            }
        "#,
        json!({
            "Book": [{
                "title": "A1",
                "publisher": { "name": "Penguin", "owner": { "name": "Acme" } },
            }],
        }),
    );
}

#[test]
#[mantra_macros::req_test("defra.index.relation.filter_keeps_joined_parent")]
fn relation_filter_through_an_indexed_fk_keeps_a_second_relation_filter() {
    expect(
        integration_test::scenario_id!(),
        r#"
            query {
                Book(filter: {
                    author: {name: {_eq: "Alice"}},
                    publisher: {name: {_eq: "Penguin"}}
                }) {
                    title
                    publisher { name }
                }
            }
        "#,
        json!({
            "Book": [{ "title": "A1", "publisher": { "name": "Penguin" } }],
        }),
    );
}

#[test]
#[mantra_macros::req_test("defra.index.relation.filter_keeps_joined_parent")]
fn relation_filter_through_an_indexed_fk_keeps_a_joined_list_relation() {
    expect(
        integration_test::scenario_id!(),
        r#"
            query {
                Book(filter: {author: {name: {_eq: "Bob"}}}) {
                    title
                    reviews { body }
                }
            }
        "#,
        json!({
            "Book": [{ "title": "B1", "reviews": [{ "body": "good" }] }],
        }),
    );
}

#[test]
#[mantra_macros::req_test("defra.index.relation.filter_keeps_joined_parent")]
fn relation_filter_through_an_indexed_fk_keeps_a_list_relation_filter() {
    expect(
        integration_test::scenario_id!(),
        r#"
            query {
                Book(filter: {
                    author: {name: {_eq: "Alice"}},
                    reviews: {body: {_eq: "dull"}}
                }) {
                    title
                    reviews { body }
                }
            }
        "#,
        json!({
            "Book": [{ "title": "A2", "reviews": [{ "body": "dull" }] }],
        }),
    );
}
