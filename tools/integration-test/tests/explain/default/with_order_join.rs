//! Generated from the conformance scenarios of tests/integration/explain/default/with_order_join_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.errors")]
fn default_explain_request_where_parent_is_ordered_by_its_related_child() {
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
                    query @explain {
                                        Author(
                                            order: {
                                                articles: {name: ASC}
                                            }
                                        ) {
                                            articles {
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(
                    r#"Argument "order" has invalid value {articles: {name: ASC}}.
In field "articles": Unknown field."#
                        .into(),
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
#[mantra_macros::req_test("go.explain.default.order")]
fn default_explain_request_with_order_field_on_parent_and_related_child() {
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
                    query @explain {
                                        Author(order: {name: ASC}) {
                                            name
                                            articles(order: {name: DESC}) {
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "selectTopNode": {
                                        "orderNode": {
                                            "selectNode": {
                                                "typeIndexJoin": {
                                                    "root": { "scanNode": {} },
                                                    "subType": {
                                                        "selectTopNode": {
                                                            "orderNode": {
                                                                "selectNode": { "scanNode": {} },
                                                            },
                                                        },
                                                    },
                                                },
                                            },
                                        },
                                    },
                                },
                            ],
                        },
                    })),
                    m::explain_target(
                        "orderNode",
                        false,
                        0,
                        json!({ "orderings": [{ "direction": "ASC", "fields": ["name"] }] })
                    ),
                    m::explain_target(
                        "orderNode",
                        false,
                        1,
                        json!({ "orderings": [{ "direction": "DESC", "fields": ["name"] }] })
                    ),
                ]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.explain.default.order")]
fn default_explain_request_with_order_field_on_related_child() {
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
                    query @explain {
                                        Author {
                                            name
                                            articles(order: {name: DESC}) {
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "selectTopNode": {
                                        "selectNode": {
                                            "typeIndexJoin": {
                                                "root": { "scanNode": {} },
                                                "subType": {
                                                    "selectTopNode": {
                                                        "orderNode": {
                                                            "selectNode": { "scanNode": {} },
                                                        },
                                                    },
                                                },
                                            },
                                        },
                                    },
                                },
                            ],
                        },
                    })),
                    m::explain_target(
                        "orderNode",
                        false,
                        0,
                        json!({ "orderings": [{ "direction": "DESC", "fields": ["name"] }] })
                    ),
                ]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
