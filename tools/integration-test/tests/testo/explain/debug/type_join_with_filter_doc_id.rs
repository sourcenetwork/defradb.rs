//! Generated from the conformance scenarios of tests/integration/explain/debug/type_join_with_filter_doc_id_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.debug.join")]
fn debug_explain_request_with_many_related_filters_and_doc_id() {
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
                                        Author(
                                            filter: {
                                                name: {_eq: "Cornelia Funke"},
                                                articles: {name: {_eq: "To my dear readers"}},
                                                books: {name: {_eq: "Theif Lord"}}
                                            },
                                            docID: ["bae-079d0bd8-4b1b-5f5f-bd95-4d915c277f9d"]
                                        ) {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!(m::explain_pattern(json!({
                    "explain": {
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "selectNode": {
                                        "parallelNode": [
                                            {
                                                "typeIndexJoin": {
                                                    "typeJoinMany": {
                                                        "root": {
                                                            "multiScanNode": { "scanNode": {} },
                                                        },
                                                        "subType": {
                                                            "selectTopNode": {
                                                                "selectNode": { "scanNode": {} },
                                                            },
                                                        },
                                                    },
                                                },
                                            },
                                            {
                                                "typeIndexJoin": {
                                                    "typeJoinMany": {
                                                        "root": {
                                                            "multiScanNode": { "scanNode": {} },
                                                        },
                                                        "subType": {
                                                            "selectTopNode": {
                                                                "selectNode": { "scanNode": {} },
                                                            },
                                                        },
                                                    },
                                                },
                                            },
                                        ],
                                    },
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
#[mantra_macros::req_test("go.explain.debug.join")]
fn debug_explain_request_with_related_and_regular_filter_and_doc_i_ds() {
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
                                        Author(
                                            filter: {
                                                name: {_eq: "John Grisham"},
                                                books: {name: {_eq: "Painted House"}}
                                            },
                                            docID: [
                                                "bae-079d0bd8-4b1b-5f5f-bd95-4d915c277f9d",
                                                "bae-079d0bd8-4b1b-5f5f-bd95-4d915c277f8e"
                                            ]
                                        ) {
                                            name
                                            age
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::explain_pattern(json!({
                    "explain": {
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "selectNode": {
                                        "typeIndexJoin": {
                                            "typeJoinMany": {
                                                "root": { "scanNode": {} },
                                                "subType": {
                                                    "selectTopNode": { "selectNode": { "scanNode": {} } },
                                                },
                                            },
                                        },
                                    },
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
