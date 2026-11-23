//! An aggregate whose filter differs from a selection of the same relation
//! gets a join of its own, so its filter never drops the selection's rows.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.index.relation.aggregate_filter_own_join")]
fn an_aggregate_filter_does_not_drop_an_indexed_child_selection() {
    let book = |title: &str, pages: i64| -> integration_test::actions::Actions {
        AddDoc {
            collection: "Book".into(),
            doc_map: Some(json!({ "title": title, "pages": pages, "author": m::r("ann") })),
            ..Default::default()
        }
        .into()
    };
    TestCase::from_actions(vec![
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
        book("A", 50),
        book("B", 200),
        Request {
            request: r#"query {
                Person {
                    name
                    authoredBooks(filter: {title: {_eq: "A"}}) { title }
                    COUNT(authoredBooks: {filter: {pages: {_gt: 100}}})
                }
            }"#
            .into(),
            expected_data: json!({
                "Person": [{ "name": "Ann", "authoredBooks": [{ "title": "A" }], "COUNT": 1 }],
            }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
