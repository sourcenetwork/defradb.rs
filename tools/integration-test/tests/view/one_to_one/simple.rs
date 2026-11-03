//! Generated from the conformance scenarios of tests/integration/view/one_to_one/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddView, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.view.one_to_one")]
fn view_one_to_one_duplicate_embedded_schema_errors() {
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
                    Author {
                        authorName: name
                        books {
                            bookName: name
                        }
                    }
                "#
                .into(),
                sdl: r#"
                    type AuthorAliasView @materialized(if: false) {
                        authorName: String
                        books: [BookView]
                    }
                    interface BookView {
                        bookName: String
                    }
                "#
                .into(),
                expected_error: Some("collection already exists. Name: BookView".into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type (name: "BookView") {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "__type": { "name": "BookView" } }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
