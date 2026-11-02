//! Generated from the conformance scenarios of tests/integration/view/one_to_many/with_count_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, AddView, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.view.one_to_many_count")]
fn view_one_to_many_with_aliased_count() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        books: [Book]
                    }
                    type Book {
                        name: String
                        author: Author
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    Author {
                        name
                        numberOfBooks: COUNT(books: {})
                    }
                "#.into(),
                sdl: r#"
                    type AuthorView @materialized(if: false) {
                        name: String
                        numberOfBooks: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Harper Lee" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "To Kill a Mockingbird" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "Go Set a Watchman" })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        AuthorView {
                            name
                            numberOfBooks
                        }
                    }
                "#.into(),
                expected_data: json!({ "AuthorView": [{ "name": "Harper Lee", "numberOfBooks": 2 }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.view.one_to_many_count")]
fn view_one_to_many_with_count_errors() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        books: [Book]
                    }
                    type Book {
                        name: String
                        author: Author
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    Author {
                        name
                        COUNT(books: {})
                    }
                "#
                .into(),
                sdl: r#"
                    type AuthorView @materialized(if: false) {
                        name: String
                        COUNT: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Harper Lee" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "name": "To Kill a Mockingbird",
                    "_authorID": "bae-ef9cd756-08e1-5f23-abeb-7b3e6351a68d",
                })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "name": "Go Set a Watchman",
                    "_authorID": "bae-ef9cd756-08e1-5f23-abeb-7b3e6351a68d",
                })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                                AuthorView {
                                                    name
                                                    COUNT
                                                }
                                            }
                "#
                .into(),
                expected_error: Some(
                    "aggregate must be provided with a property to aggregate".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.view.one_to_many_count")]
fn view_one_to_many_with_count_in_query_but_not_sdl() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        books: [Book]
                    }
                    type Book {
                        name: String
                        author: Author
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    Author {
                        name
                        COUNT(books: {})
                    }
                "#
                .into(),
                sdl: r#"
                    type AuthorView @materialized(if: false) {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Harper Lee" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        AuthorView {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "AuthorView": [{ "name": "Harper Lee" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
