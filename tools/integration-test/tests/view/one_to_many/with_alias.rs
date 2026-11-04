//! Generated from the conformance scenarios of tests/integration/view/one_to_many/with_alias_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, AddView, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.view.one_to_many")]
fn view_one_to_many_with_alias_on_inner() {
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
                            fullName: name
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
                        fullName: String
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
                                fullName
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "AuthorView": [
                        {
                            "books": [{ "fullName": "To Kill a Mockingbird" }],
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
fn view_one_to_many_with_alias_on_outer() {
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
                        fullName: name
                        books {
                            name
                        }
                    }
                "#
                .into(),
                sdl: r#"
                    type AuthorView @materialized(if: false) {
                        fullName: String
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
                                                    fullName
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
                            "fullName": "Harper Lee",
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
