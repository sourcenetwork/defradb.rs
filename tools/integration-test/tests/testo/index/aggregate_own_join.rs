//! An aggregate whose filter differs from a selection of the same relation
//! gets a join of its own, so its filter never drops the selection's rows.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn run_on_ann_books(books: &[(&str, i64)], request: &str, expected_data: Value) {
    let mut actions: Vec<Actions> = vec![
        AddCollection {
            sdl: r#"
                type Person {
                    name: String
                    authoredBooks: [Book]
                }
                type Book {
                    title: String @index
                    pages: Int
                    author: Person
                }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Person".into(),
            doc_map: Some(json!({ "name": "Ann" })),
            common: Common {
                capture: captures([("ann", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ];
    for (title, pages) in books {
        actions.push(
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": title, "pages": pages, "author": m::r("ann") })),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        Request {
            request: request.into(),
            expected_data,
            ..Default::default()
        }
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.relation.aggregate_filter_own_join")]
fn an_aggregate_filter_does_not_drop_an_indexed_child_selection() {
    run_on_ann_books(
        &[("A", 50), ("B", 200)],
        r#"query {
            Person {
                name
                authoredBooks(filter: {title: {_eq: "A"}}) { title }
                COUNT(authoredBooks: {filter: {pages: {_gt: 100}}})
            }
        }"#,
        json!({
            "Person": [{ "name": "Ann", "authoredBooks": [{ "title": "A" }], "COUNT": 1 }],
        }),
    );
}

#[test]
#[mantra_macros::req_test("defra.index.relation.aggregate_filter_own_join")]
fn a_selection_limit_does_not_cut_an_aggregate_with_the_same_filter() {
    run_on_ann_books(
        &[("A", 50), ("B", 200), ("C", 5)],
        r#"query {
            Person {
                name
                authoredBooks(filter: {pages: {_gt: 10}}, order: {title: ASC}, limit: 1) { title }
                COUNT(authoredBooks: {filter: {pages: {_gt: 10}}})
            }
        }"#,
        json!({
            "Person": [{ "name": "Ann", "authoredBooks": [{ "title": "A" }], "COUNT": 2 }],
        }),
    );
}
