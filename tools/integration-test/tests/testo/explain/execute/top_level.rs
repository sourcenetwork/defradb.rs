//! Generated from the conformance scenarios of tests/integration/explain/execute/top_level_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.execute.aggregate")]
fn execute_explain_top_level_average_request() {
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
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 64 })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 48 })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        AVG(
                                            Author: {
                                                field: age
                                            }
                                        )
                                    }
                "#
                .into(),
                expected_data: json!({
                    "explain": {
                        "executionSuccess": true,
                        "operationNode": [
                            {
                                "topLevelNode": [
                                    {
                                        "selectTopNode": {
                                            "selectNode": {
                                                "filterMatches": 2,
                                                "iterations": 3,
                                                "scanNode": {
                                                    "docFetches": 2,
                                                    "fieldFetches": 4,
                                                    "indexFetches": 0,
                                                    "iterations": 3,
                                                },
                                            },
                                        },
                                    },
                                    { "sumNode": { "iterations": 1 } },
                                    { "countNode": { "iterations": 1 } },
                                    { "averageNode": { "iterations": 1 } },
                                ],
                            },
                        ],
                        "planExecutions": 2,
                        "sizeOfResult": 1,
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

#[test]
#[mantra_macros::req_test("go.explain.execute.aggregate")]
fn execute_explain_top_level_count_request() {
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
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 64 })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 48 })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        COUNT(Author: {})
                                    }
                "#
                .into(),
                expected_data: json!({
                    "explain": {
                        "executionSuccess": true,
                        "operationNode": [
                            {
                                "topLevelNode": [
                                    {
                                        "selectTopNode": {
                                            "selectNode": {
                                                "filterMatches": 2,
                                                "iterations": 3,
                                                "scanNode": {
                                                    "docFetches": 2,
                                                    "fieldFetches": 4,
                                                    "indexFetches": 0,
                                                    "iterations": 3,
                                                },
                                            },
                                        },
                                    },
                                    { "countNode": { "iterations": 1 } },
                                ],
                            },
                        ],
                        "planExecutions": 2,
                        "sizeOfResult": 1,
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

#[test]
#[mantra_macros::req_test("go.explain.execute.aggregate")]
fn execute_explain_top_level_sum_request() {
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
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 64 })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 48 })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        SUM(
                                            Author: {
                                                field: age
                                            }
                                        )
                                    }
                "#
                .into(),
                expected_data: json!({
                    "explain": {
                        "executionSuccess": true,
                        "operationNode": [
                            {
                                "topLevelNode": [
                                    {
                                        "selectTopNode": {
                                            "selectNode": {
                                                "filterMatches": 2,
                                                "iterations": 3,
                                                "scanNode": {
                                                    "docFetches": 2,
                                                    "fieldFetches": 4,
                                                    "indexFetches": 0,
                                                    "iterations": 3,
                                                },
                                            },
                                        },
                                    },
                                    { "sumNode": { "iterations": 1 } },
                                ],
                            },
                        ],
                        "planExecutions": 2,
                        "sizeOfResult": 1,
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
