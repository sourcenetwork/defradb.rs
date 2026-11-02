//! Generated from the conformance scenarios of tests/integration/view/one_to_many/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, AddView, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.view.one_to_many")]
fn view_one_to_many_from_inner_side_errors() {
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
                        books {
                            name
                        }
                    }
                "#
                .into(),
                sdl: r#"
                    type AuthorView @materialized(if: false) {
                        name: String
                        books: [BookView]
                    }
                    interface BookView {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                                BookView {
                                                    name
                                                    author {
                                                        name
                                                    }
                                                }
                                            }
                "#
                .into(),
                expected_error: Some(r#"Cannot query field "BookView" on type "Query"."#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.view.one_to_many")]
fn view_one_to_many_multiple_views_with_embedded_schema() {
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
                    Book {
                        name
                        author {
                            name
                        }
                    }
                "#
                .into(),
                sdl: r#"
                    type BookView @materialized(if: false) {
                        name: String
                        author: AuthorView
                    }
                    interface AuthorView {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    Book {
                        name
                        author {
                            name
                        }
                    }
                "#
                .into(),
                sdl: r#"
                    type BookView2 @materialized(if: false) {
                        name: String
                        author: AuthorView2
                    }
                    interface AuthorView2 {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.view.one_to_many")]
fn view_one_to_many_outer_to_inner_to_outer_errors() {
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
                        books {
                            name
                            author {
                                name
                            }
                        }
                    }
                "#
                .into(),
                sdl: r#"
                    type AuthorView @materialized(if: false) {
                        name: String
                        books: [BookView]
                    }
                    interface BookView {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                                AuthorView {
                                                    name
                                                    books {
                                                        name
                                                        author {
                                                            name
                                                        }
                                                    }
                                                }
                                            }
                "#
                .into(),
                expected_error: Some(r#"Cannot query field "author" on type "BookView"."#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.view.one_to_many")]
fn view_one_to_many_with_double_sided_relation_errors() {
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
                        books {
                            name
                        }
                    }
                "#
                .into(),
                sdl: r#"
                    type AuthorView @materialized(if: false) {
                        name: String
                        books: [BookView]
                    }
                    interface BookView {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    AuthorView {
                        name
                        books {
                            name
                        }
                    }
                "#
                .into(),
                sdl: r#"
                    type AuthorViewView @materialized(if: false) {
                        name: String
                        books: [BookViewView]
                    }
                    interface BookViewView {
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
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "_authorID": m::r("doc0_0"), "name": "To Kill a Mockingbird" }),
                ),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                                AuthorViewView {
                                                    name
                                                    books {
                                                        name
                                                    }
                                                }
                                            }
                "#
                .into(),
                expected_data: json!({
                    "AuthorViewView": [
                        {
                            "books": [{ "name": "To Kill a Mockingbird" }],
                            "name": "Harper Lee",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.view.one_to_many")]
fn view_one_to_many_with_mixed_sdl() {
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
                        books {
                            name
                        }
                    }
                "#
                .into(),
                sdl: r#"
                    type AuthorView @materialized(if: false) {
                        name: String
                        books: [Book]
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
                doc_map: Some(
                    json!({ "_authorID": m::r("doc0_0"), "name": "To Kill a Mockingbird" }),
                ),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
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
                                                    books {
                                                        name
                                                    }
                                                }
                                            }
                "#
                .into(),
                expected_data: json!({
                    "AuthorView": [
                        {
                            "books": [{ "name": "To Kill a Mockingbird" }],
                            "name": "Harper Lee",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.view.one_to_many")]
fn view_one_to_many_with_relation_in_query_but_not_in_sdl() {
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
                        books {
                            name
                        }
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

#[test]
#[mantra_macros::req_test("go.view.one_to_many")]
fn view_one_to_many() {
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
                        books {
                            name
                        }
                    }
                "#
                .into(),
                sdl: r#"
                    type AuthorView @materialized(if: false) {
                        name: String
                        books: [BookView]
                    }
                    interface BookView {
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
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "_authorID": m::r("doc0_0"), "name": "To Kill a Mockingbird" }),
                ),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
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
                                                    books {
                                                        name
                                                    }
                                                }
                                            }
                "#
                .into(),
                expected_data: json!({
                    "AuthorView": [
                        {
                            "books": [{ "name": "To Kill a Mockingbird" }],
                            "name": "Harper Lee",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
