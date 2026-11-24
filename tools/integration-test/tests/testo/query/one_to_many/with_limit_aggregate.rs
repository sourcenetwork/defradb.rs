use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;
use testo::action::ActionInstance;

type Action = ActionInstance<Actions>;

fn indexed_books() -> Vec<Action> {
    let book = |name: &str, rating: f64, author: &str| -> Action {
        AddDoc {
            collection: "Book".into(),
            doc_map: Some(json!({
                "name": name,
                "rating": rating,
                "_authorID": m::r(author)
            })),
            ..Default::default()
        }
        .into()
    };
    vec![
        AddCollection {
            sdl: r#"
                type Book {
                    name: String
                    rating: Float @index
                    author: Author
                }

                type Author {
                    name: String
                    published: [Book]
                }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Author".into(),
            doc_map: Some(json!({ "name": "A" })),
            common: Common {
                capture: captures([("a", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Author".into(),
            doc_map: Some(json!({ "name": "B" })),
            common: Common {
                capture: captures([("b", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        book("a3", 4.0, "a"),
        book("a2", 2.5, "a"),
        book("a1", 1.5, "a"),
        book("b2", 3.5, "b"),
        book("b1", 3.0, "b"),
    ]
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many.aggregate.sum-ordered-paging")]
fn query_one_to_many_with_indexed_child_limit_keeps_a_sibling_sum_whole() {
    let mut actions = indexed_books();
    actions.push(
        Request {
            request: r#"
                query {
                    Author {
                        name
                        published(order: {rating: ASC}, limit: 1) {
                            name
                        }
                        SUM(published: {field: rating})
                    }
                }
            "#
            .into(),
            expected_data: json!({
                "Author": [
                    { "name": "A", "published": [{ "name": "a1" }], "SUM": 8.0 },
                    { "name": "B", "published": [{ "name": "b1" }], "SUM": 6.5 },
                ],
            }),
            common: Common {
                compare: Some(true),
                unordered_documents: vec!["/data/Author".into()],
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        id: integration_test::scenario_id!(),
        actions,
        ..Default::default()
    }
    .execute();
}
