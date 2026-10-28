//! Generated from the conformance scenarios of tests/integration/explain/debug/with_average_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.debug.aggregate.inline_array")]
fn debug_explain_request_with_average_on_array_field() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Article {
                        name: String
                        author: Author
                        pages: Int
                    }

                    type Book {
                        name: String
                        author: Author
                        rating: Float
                        pages: Int
                        chapterPages: [Int!]
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        books: [Book]
                        articles: [Article]
                        contact: AuthorContact @primary
                    }

                    type AuthorContact {
                        cell: String
                        email: String
                        author: Author
                        address: ContactAddress @primary
                    }

                    type ContactAddress {
                        city: String
                        country: String
                        contact: AuthorContact
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: debug) {
                                        Book {
                                            name
                                            AVG(chapterPages: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "explain": {
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "averageNode": {
                                        "countNode": {
                                            "sumNode": { "selectNode": { "scanNode": {} } },
                                        },
                                    },
                                },
                            },
                        ],
                    },
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
