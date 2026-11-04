//! Generated from the conformance scenarios of tests/integration/explain/debug/group_with_order_child_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.debug.group.limit_order")]
fn debug_explain_request_with_ascending_order_on_inner_group_selection() {
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
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: debug) {
                                        Author(groupBy: [name]) {
                                            name
                                            GROUP (order: {age: ASC}){
                                                age
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::explain_pattern(json!({
                    "explain": {
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "groupNode": { "selectNode": { "pipeNode": { "scanNode": {} } } },
                                },
                            },
                        ],
                    },
                }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.explain.debug.group.limit_order")]
fn debug_explain_request_with_descending_order_on_inner_group_selection() {
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
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: debug) {
                                        Author(groupBy: [name]) {
                                            name
                                            GROUP (order: {age: DESC}){
                                                age
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::explain_pattern(json!({
                    "explain": {
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "groupNode": { "selectNode": { "pipeNode": { "scanNode": {} } } },
                                },
                            },
                        ],
                    },
                }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.explain.debug.group.limit_order")]
fn debug_explain_request_with_order_on_nested_parent_group_by_and_on_nested_parents_inner_group_selection(
) {
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
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: debug) {
                                        Author(groupBy: [name]) {
                                            name
                                            GROUP (
                                                groupBy: [verified],
                                                order: {verified: ASC}
                                            ){
                                                verified
                                                GROUP (order: {age: DESC}) {
                                                    age
                                                }
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::explain_pattern(json!({
                    "explain": {
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "groupNode": { "selectNode": { "pipeNode": { "scanNode": {} } } },
                                },
                            },
                        ],
                    },
                }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
