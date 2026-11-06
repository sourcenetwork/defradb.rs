//! Generated from the conformance scenarios of tests/integration/explain/execute/with_sum_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.execute.aggregate")]
fn execute_explain_request_sum_of_related_one_to_many_field() {
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
                collection: "ContactAddress".into(),
                doc_map: Some(json!({ "city": "Waterloo", "country": "Canada" })),
                common: Common {
                    capture: captures([("doc4_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "ContactAddress".into(),
                doc_map: Some(json!({ "city": "Brampton", "country": "Canada" })),
                common: Common {
                    capture: captures([("doc4_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "AuthorContact".into(),
                doc_map: Some(json!({
                    "_addressID": m::r("doc4_0"),
                    "cell": "5197212301",
                    "email": "john_grisham@example.com",
                })),
                common: Common {
                    capture: captures([("doc3_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "AuthorContact".into(),
                doc_map: Some(json!({
                    "_addressID": m::r("doc4_1"),
                    "cell": "5197212302",
                    "email": "cornelia_funke@example.com",
                })),
                common: Common {
                    capture: captures([("doc3_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "_contactID": m::r("doc3_0"),
                    "age": 65,
                    "name": "John Grisham",
                    "verified": true,
                })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "_contactID": m::r("doc3_1"),
                    "age": 62,
                    "name": "Cornelia Funke",
                    "verified": false,
                })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Article".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc2_0"),
                    "name": "After Guantánamo, Another Injustice",
                    "pages": 100,
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Article".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc2_1"),
                    "name": "To my dear readers",
                    "pages": 200,
                })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Article".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc2_1"),
                    "name": "Twinklestar's Favourite Xmas Cookie",
                    "pages": 300,
                })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        Author {
                                            name
                                            TotalPages: SUM(
                                                articles: {
                                                    field: pages,
                                                }
                                            )
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "explain": {
                        "executionSuccess": true,
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "sumNode": {
                                        "iterations": 3,
                                        "selectNode": {
                                            "filterMatches": 2,
                                            "iterations": 3,
                                            "typeIndexJoin": {
                                                "iterations": 3,
                                                "typeJoinMany": {
                                                    "root": {
                                                        "scanNode": {
                                                            "docFetches": 2,
                                                            "fieldFetches": 8,
                                                            "indexFetches": 0,
                                                            "iterations": 3,
                                                        },
                                                    },
                                                    "subType": {
                                                        "selectTopNode": {
                                                            "selectNode": {
                                                                "filterMatches": 3,
                                                                "iterations": 5,
                                                                "scanNode": {
                                                                    "docFetches": 6,
                                                                    "fieldFetches": 18,
                                                                    "indexFetches": 0,
                                                                    "iterations": 5,
                                                                },
                                                            },
                                                        },
                                                    },
                                                },
                                            },
                                        },
                                    },
                                },
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
fn execute_explain_request_with_sum_of_inline_array_field() {
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
            AddDoc {
                collection: "ContactAddress".into(),
                doc_map: Some(json!({ "city": "Waterloo", "country": "Canada" })),
                common: Common { capture: captures([("doc4_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "ContactAddress".into(),
                doc_map: Some(json!({ "city": "Brampton", "country": "Canada" })),
                common: Common { capture: captures([("doc4_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "AuthorContact".into(),
                doc_map: Some(json!({
                    "_addressID": m::r("doc4_0"),
                    "cell": "5197212301",
                    "email": "john_grisham@example.com",
                })),
                common: Common { capture: captures([("doc3_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "AuthorContact".into(),
                doc_map: Some(json!({
                    "_addressID": m::r("doc4_1"),
                    "cell": "5197212302",
                    "email": "cornelia_funke@example.com",
                })),
                common: Common { capture: captures([("doc3_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "_contactID": m::r("doc3_0"),
                    "age": 65,
                    "name": "John Grisham",
                    "verified": true,
                })),
                common: Common { capture: captures([("doc2_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "_contactID": m::r("doc3_1"),
                    "age": 62,
                    "name": "Cornelia Funke",
                    "verified": false,
                })),
                common: Common { capture: captures([("doc2_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc2_0"),
                    "chapterPages": [1, 22, 33, 44, 55, 66],
                    "name": "Painted House",
                    "pages": 78,
                })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc2_0"),
                    "chapterPages": [0, 22, 101, 321],
                    "name": "A Time for Mercy",
                    "pages": 333,
                })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc2_1"), "name": "Theif Lord", "pages": 20 })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        Book {
                                            name
                                            NotSureWhySomeoneWouldSumTheChapterPagesButHereItIs: SUM(chapterPages: {})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "explain": {
                        "executionSuccess": true,
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "sumNode": {
                                        "iterations": 4,
                                        "selectNode": {
                                            "filterMatches": 3,
                                            "iterations": 4,
                                            "scanNode": {
                                                "docFetches": 3,
                                                "fieldFetches": 11,
                                                "indexFetches": 0,
                                                "iterations": 4,
                                            },
                                        },
                                    },
                                },
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
