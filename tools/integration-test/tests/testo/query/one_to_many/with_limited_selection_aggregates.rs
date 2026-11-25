use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};
use testo::action::ActionInstance;

type Action = ActionInstance<Actions>;

fn author_with_books() -> Vec<Action> {
    let book = |title: &str, pages: i64, rating: f64| -> Action {
        AddDoc {
            collection: "Book".into(),
            doc_map: Some(json!({
                "title": title,
                "pages": pages,
                "rating": rating,
                "_authorID": m::r("author")
            })),
            ..Default::default()
        }
        .into()
    };
    vec![
        AddCollection {
            sdl: r#"
                type Book {
                    title: String
                    pages: Int
                    rating: Float
                    author: Author
                }

                type Author {
                    name: String
                    authoredBooks: [Book]
                }
            "#
            .into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Author".into(),
            doc_map: Some(json!({ "name": "Ann" })),
            common: Common {
                capture: captures([("author", "/docID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        book("Short", 80, 1.0),
        book("Mid", 120, 2.0),
        book("Long", 150, 4.0),
        book("Epic", 330, 3.0),
    ]
}

fn run(request: &str, expected: Value) {
    let mut actions = author_with_books();
    actions.push(
        Request {
            request: request.into(),
            expected_data: json!({ "Author": [expected] }),
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

#[test]
#[mantra_macros::req_test("defra.query.one_to_many.aggregate.beside-paged-selection")]
fn aggregates_after_count_ignore_a_limited_selection() {
    run(
        r#"
            query {
                Author {
                    authoredBooks(filter: {pages: {_gt: 100}}, limit: 1) {
                        title
                    }
                    COUNT(authoredBooks: {filter: {pages: {_gt: 100}}})
                    SUM(authoredBooks: {field: pages, filter: {pages: {_gt: 100}}})
                    AVG(authoredBooks: {field: pages, filter: {pages: {_gt: 100}}})
                    MIN(authoredBooks: {field: pages, filter: {pages: {_gt: 100}}})
                    MAX(authoredBooks: {field: pages, filter: {pages: {_gt: 100}}})
                }
            }
        "#,
        json!({
            "authoredBooks": m::length(1),
            "COUNT": 3,
            "SUM": 600,
            "AVG": 200.0,
            "MIN": 120,
            "MAX": 330,
        }),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_many.aggregate.beside-paged-selection")]
fn aggregates_after_count_ignore_an_offset_selection() {
    run(
        r#"
            query {
                Author {
                    authoredBooks(filter: {pages: {_gt: 100}}, offset: 2) {
                        title
                    }
                    COUNT(authoredBooks: {filter: {pages: {_gt: 100}}})
                    SUM(authoredBooks: {field: pages, filter: {pages: {_gt: 100}}})
                    MIN(authoredBooks: {field: pages, filter: {pages: {_gt: 100}}})
                }
            }
        "#,
        json!({
            "authoredBooks": m::length(1),
            "COUNT": 3,
            "SUM": 600,
            "MIN": 120,
        }),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_many.aggregate.beside-paged-selection")]
fn aggregates_after_count_ignore_an_ordered_limited_selection() {
    run(
        r#"
            query {
                Author {
                    authoredBooks(filter: {pages: {_gt: 100}}, order: {pages: DESC}, limit: 2) {
                        title
                    }
                    COUNT(authoredBooks: {filter: {pages: {_gt: 100}}})
                    AVG(authoredBooks: {field: rating, filter: {pages: {_gt: 100}}})
                    MAX(authoredBooks: {field: rating, filter: {pages: {_gt: 100}}})
                }
            }
        "#,
        json!({
            "authoredBooks": [{ "title": "Epic" }, { "title": "Long" }],
            "COUNT": 3,
            "AVG": 3.0,
            "MAX": 4.0,
        }),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_many.aggregate.beside-paged-selection")]
fn aggregates_without_count_ignore_a_limited_selection() {
    run(
        r#"
            query {
                Author {
                    authoredBooks(filter: {pages: {_gt: 100}}, order: {pages: DESC}, limit: 1) {
                        pages
                    }
                    SUM(authoredBooks: {field: pages, filter: {pages: {_gt: 100}}})
                    AVG(authoredBooks: {field: pages, filter: {pages: {_gt: 100}}})
                    MIN(authoredBooks: {field: pages, filter: {pages: {_gt: 100}}})
                    MAX(authoredBooks: {field: pages, filter: {pages: {_gt: 100}}})
                }
            }
        "#,
        json!({
            "authoredBooks": [{ "pages": 330 }],
            "SUM": 600,
            "AVG": 200.0,
            "MIN": 120,
            "MAX": 330,
        }),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_many.aggregate.beside-paged-selection")]
fn aggregates_with_different_filters_read_their_own_rows() {
    run(
        r#"
            query {
                Author {
                    authoredBooks(order: {pages: ASC}, limit: 1) {
                        title
                    }
                    COUNT(authoredBooks: {filter: {pages: {_gt: 100}}})
                    SUM(authoredBooks: {field: pages, filter: {pages: {_gt: 140}}})
                    MIN(authoredBooks: {field: pages})
                    MAX(authoredBooks: {field: rating, filter: {pages: {_lt: 200}}})
                }
            }
        "#,
        json!({
            "authoredBooks": [{ "title": "Short" }],
            "COUNT": 3,
            "SUM": 480,
            "MIN": 80,
            "MAX": 4.0,
        }),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_many.aggregate.beside-paged-selection")]
fn aggregates_with_different_filters_and_no_selection_read_their_own_rows() {
    run(
        r#"
            query {
                Author {
                    COUNT(authoredBooks: {filter: {pages: {_gt: 100}}})
                    SUM(authoredBooks: {field: pages, filter: {pages: {_gt: 140}}})
                    AVG(authoredBooks: {field: rating, filter: {pages: {_gt: 100}}})
                    MIN(authoredBooks: {field: pages})
                }
            }
        "#,
        json!({
            "COUNT": 3,
            "SUM": 480,
            "AVG": 3.0,
            "MIN": 80,
        }),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.one_to_many.aggregate.beside-paged-selection")]
fn unfiltered_aggregates_ignore_a_filtered_selection() {
    run(
        r#"
            query {
                Author {
                    authoredBooks(filter: {pages: {_gt: 100}}, order: {pages: ASC}) {
                        title
                    }
                    SUM(authoredBooks: {field: pages})
                    MIN(authoredBooks: {field: pages})
                }
            }
        "#,
        json!({
            "authoredBooks": [{ "title": "Mid" }, { "title": "Long" }, { "title": "Epic" }],
            "SUM": 680,
            "MIN": 80,
        }),
    );
}
