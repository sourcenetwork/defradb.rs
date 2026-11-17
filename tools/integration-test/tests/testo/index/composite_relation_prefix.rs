//! A composite index that starts with a relation field serves a join or a
//! filter through that relation by its first field alone
//! (`defra.index.composite_relation_prefix`).

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

/// Two authors, Tolkien with two books so ordering shows. The index on `name`
/// lets a filter through `author` start from the author and join back.
fn library(index: &str) -> Vec<integration_test::actions::Actions> {
    let mut actions = vec![AddCollection {
        sdl: format!(
            "type Author {{ name: String @index books: [Book] }}
             type Book {index} {{ title: String pages: Int author: Author }}"
        )
        .into(),
        ..Default::default()
    }
    .into()];
    for (name, key) in [("Tolkien", "tolkien"), ("Herbert", "herbert")] {
        actions.push(
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": name })),
                common: Common {
                    capture: captures([(key, "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        );
    }
    for (title, pages, author) in [
        ("The Hobbit", 310, "tolkien"),
        ("Silmarillion", 365, "tolkien"),
        ("Dune", 412, "herbert"),
    ] {
        actions.push(
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": title, "pages": pages, "author": m::r(author) })),
                ..Default::default()
            }
            .into(),
        );
    }
    actions
}

fn request(query: &str, expected: Value) -> integration_test::actions::Actions {
    Request {
        request: format!("query {{ {query} }}").into(),
        expected_data: expected,
        ..Default::default()
    }
    .into()
}

/// The queries a relation can route through the index.
fn queries() -> Vec<integration_test::actions::Actions> {
    vec![
        request(
            r#"Book(filter: {author: {name: {_eq: "Herbert"}}}) { title }"#,
            json!({ "Book": [{ "title": "Dune" }] }),
        ),
        request(
            r#"Book(filter: {author: {name: {_gt: "J"}}}, order: {title: ASC}) { title }"#,
            json!({ "Book": [{ "title": "Silmarillion" }, { "title": "The Hobbit" }] }),
        ),
        request(
            "Book(order: [{author: {name: ASC}}, {title: ASC}]) { title }",
            json!({ "Book": [
                { "title": "Dune" },
                { "title": "Silmarillion" },
                { "title": "The Hobbit" },
            ] }),
        ),
        request(
            r#"Author(filter: {books: {title: {_eq: "Dune"}}}) { name }"#,
            json!({ "Author": [{ "name": "Herbert" }] }),
        ),
        request(
            r#"Author(filter: {books: {pages: {_gt: 400}}}) { name }"#,
            json!({ "Author": [{ "name": "Herbert" }] }),
        ),
        request(
            r#"Author(order: {name: ASC}) { name books(order: {title: ASC}) { title } }"#,
            json!({ "Author": [
                { "name": "Herbert", "books": [{ "title": "Dune" }] },
                { "name": "Tolkien", "books": [{ "title": "Silmarillion" }, { "title": "The Hobbit" }] },
            ] }),
        ),
        request(
            r#"Author(filter: {name: {_eq: "Tolkien"}}) { books(filter: {pages: {_lt: 350}}) { title } }"#,
            json!({ "Author": [{ "books": [{ "title": "The Hobbit" }] }] }),
        ),
    ]
}

fn relation_queries(index: &str) {
    let mut actions = library(index);
    actions.extend(queries());
    TestCase::from_actions(actions).execute();
}

/// One query against the plain composite index, so each failure shows alone.
fn one_query(position: usize) {
    let mut actions = library(r#"@index(includes: [{field: "author"}, {field: "title"}])"#);
    actions.push(queries().swap_remove(position));
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.composite_relation_prefix")]
fn a_composite_index_led_by_the_relation_serves_the_relation() {
    relation_queries(r#"@index(includes: [{field: "author"}, {field: "title"}])"#);
}

#[test]
#[mantra_macros::req_test("defra.index.composite_relation_prefix")]
fn a_composite_index_led_by_the_relation_id_serves_the_relation() {
    relation_queries(r#"@index(includes: [{field: "_authorID"}, {field: "pages"}])"#);
}

#[test]
#[mantra_macros::req_test("defra.index.composite_relation_prefix")]
fn a_unique_composite_index_led_by_the_relation_serves_the_relation() {
    relation_queries(r#"@index(unique: true, includes: [{field: "author"}, {field: "title"}])"#);
}

#[test]
#[mantra_macros::req_test("defra.index.composite_relation_prefix")]
fn a_descending_composite_index_led_by_the_relation_serves_the_relation() {
    relation_queries(
        r#"@index(includes: [{field: "author", direction: DESC}, {field: "pages", direction: DESC}])"#,
    );
}

#[test]
#[mantra_macros::req_test("defra.index.composite_relation_prefix")]
fn a_three_field_composite_index_led_by_the_relation_serves_the_relation() {
    relation_queries(
        r#"@index(includes: [{field: "author"}, {field: "pages", direction: DESC}, {field: "title"}])"#,
    );
}

#[test]
#[mantra_macros::req_test("defra.index.composite_relation_prefix")]
fn an_eq_filter_through_the_relation_finds_the_book() {
    one_query(0);
}

#[test]
#[mantra_macros::req_test("defra.index.composite_relation_prefix")]
fn a_range_filter_through_the_relation_finds_the_books() {
    one_query(1);
}

#[test]
#[mantra_macros::req_test("defra.index.composite_relation_prefix")]
fn an_order_through_the_relation_lists_every_book() {
    one_query(2);
}

#[test]
#[mantra_macros::req_test("defra.index.composite_relation_prefix")]
fn an_eq_filter_on_the_books_finds_the_author() {
    one_query(3);
}

#[test]
#[mantra_macros::req_test("defra.index.composite_relation_prefix")]
fn a_range_filter_on_the_books_finds_the_author() {
    one_query(4);
}

#[test]
#[mantra_macros::req_test("defra.index.composite_relation_prefix")]
fn an_ordered_join_lists_each_authors_books() {
    one_query(5);
}

#[test]
#[mantra_macros::req_test("defra.index.composite_relation_prefix")]
fn a_filtered_join_lists_the_matching_books() {
    one_query(6);
}

#[test]
#[mantra_macros::req_test("defra.index.composite_relation_prefix")]
fn a_later_order_sorts_books_the_index_holds_in_another_order() {
    let mut actions = library(r#"@index(includes: [{field: "author"}, {field: "pages"}])"#);
    actions.push(queries().swap_remove(2));
    TestCase::from_actions(actions).execute();
}
