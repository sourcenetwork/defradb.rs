//! A relation filter through a parent FK index keeps other joined relations.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::captures;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.index.relation.filter_keeps_joined_parent")]
fn relation_filter_through_an_indexed_fk_keeps_another_joined_relation() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String @index
                        books: [Book]
                    }
                    type Publisher {
                        name: String
                        books: [Book]
                    }
                    type Book {
                        title: String
                        author: Author @index
                        publisher: Publisher
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: integration_test::scenario::Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                common: integration_test::scenario::Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "Penguin" })),
                common: integration_test::scenario::Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "title": "Alice's book", "author": m::r("doc0_0"), "publisher": m::r("doc1_0") }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "title": "Bob's book", "author": m::r("doc0_1"), "publisher": m::r("doc1_0") }),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Book(filter: {author: {name: {_eq: "Alice"}}}) {
                            title
                            publisher { name }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [{
                        "title": "Alice's book",
                        "publisher": { "name": "Penguin" },
                    }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
