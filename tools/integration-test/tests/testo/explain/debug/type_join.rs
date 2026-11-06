//! Generated from the conformance scenarios of tests/integration/explain/debug/type_join_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.debug.join")]
fn debug_explain_request_with2_single_joins_and1_many_join() {
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
                                        Author {
                                            OnlyEmail: contact {
                                                email
                                            }
                                            articles {
                                                name
                                            }
                                            contact {
                                                cell
                                                email
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "explain": {
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "selectNode": {
                                        "parallelNode": [
                                            {
                                                "typeIndexJoin": {
                                                    "typeJoinOne": {
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
                                            {
                                                "typeIndexJoin": {
                                                    "typeJoinOne": {
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
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
