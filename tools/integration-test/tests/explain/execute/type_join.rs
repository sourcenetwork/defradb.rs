//! Generated from the conformance scenarios of tests/integration/explain/execute/type_join_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.execute.join")]
fn execute_explain_request_with_a_one_to_one_join() {
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
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        Author {
                                            OnlyEmail: contact {
                                                email
                                            }
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
                                    "selectNode": {
                                        "filterMatches": 2,
                                        "iterations": 3,
                                        "typeIndexJoin": {
                                            "iterations": 3,
                                            "typeJoinOne": {
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
                                                            "filterMatches": 2,
                                                            "iterations": 2,
                                                            "scanNode": {
                                                                "docFetches": 2,
                                                                "fieldFetches": 6,
                                                                "indexFetches": 0,
                                                                "iterations": 2,
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
#[mantra_macros::req_test("go.explain.execute.join")]
fn execute_explain_with_multiple_one_to_one_joins() {
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
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        Author {
                                            OnlyEmail: contact {
                                                email
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
                        "executionSuccess": true,
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "selectNode": {
                                        "filterMatches": 2,
                                        "iterations": 3,
                                        "parallelNode": [
                                            {
                                                "typeIndexJoin": {
                                                    "iterations": 3,
                                                    "typeJoinOne": {
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
                                                                    "filterMatches": 2,
                                                                    "iterations": 2,
                                                                    "scanNode": {
                                                                        "docFetches": 2,
                                                                        "fieldFetches": 6,
                                                                        "indexFetches": 0,
                                                                        "iterations": 2,
                                                                    },
                                                                },
                                                            },
                                                        },
                                                    },
                                                },
                                            },
                                            {
                                                "typeIndexJoin": {
                                                    "iterations": 3,
                                                    "typeJoinOne": {
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
                                                                    "filterMatches": 2,
                                                                    "iterations": 2,
                                                                    "scanNode": {
                                                                        "docFetches": 2,
                                                                        "fieldFetches": 6,
                                                                        "indexFetches": 0,
                                                                        "iterations": 2,
                                                                    },
                                                                },
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
#[mantra_macros::req_test("go.explain.execute.join")]
fn execute_explain_with_one_to_one_join_from_secondary_side_should_include_index() {
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
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        ContactAddress {
                                            city
                                            contact {
                                                email
                                            }
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
                                    "selectNode": {
                                        "filterMatches": 2,
                                        "iterations": 3,
                                        "typeIndexJoin": {
                                            "iterations": 3,
                                            "typeJoinOne": {
                                                "root": {
                                                    "scanNode": {
                                                        "docFetches": 2,
                                                        "fieldFetches": 4,
                                                        "indexFetches": 0,
                                                        "iterations": 3,
                                                    },
                                                },
                                                "subType": {
                                                    "selectTopNode": {
                                                        "selectNode": {
                                                            "filterMatches": 2,
                                                            "iterations": 4,
                                                            "scanNode": {
                                                                "docFetches": 2,
                                                                "fieldFetches": 6,
                                                                "indexFetches": 2,
                                                                "iterations": 4,
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
#[mantra_macros::req_test("go.explain.execute.join")]
fn execute_explain_with_two_level_deep_nested_joins() {
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
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        Author {
                                            name
                                            contact {
                                                email
                                                address {
                                                    city
                                                }
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "explain": {
                        "executionSuccess": true,
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "selectNode": {
                                        "filterMatches": 2,
                                        "iterations": 3,
                                        "typeIndexJoin": {
                                            "iterations": 3,
                                            "typeJoinOne": {
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
                                                            "filterMatches": 2,
                                                            "iterations": 2,
                                                            "typeIndexJoin": {
                                                                "iterations": 2,
                                                                "typeJoinOne": {
                                                                    "root": {
                                                                        "scanNode": {
                                                                            "docFetches": 2,
                                                                            "fieldFetches": 6,
                                                                            "indexFetches": 0,
                                                                            "iterations": 2,
                                                                        },
                                                                    },
                                                                    "subType": {
                                                                        "selectTopNode": {
                                                                            "selectNode": {
                                                                                "filterMatches": 2,
                                                                                "iterations": 2,
                                                                                "scanNode": {
                                                                                    "docFetches": 2,
                                                                                    "fieldFetches": 4,
                                                                                    "indexFetches": 0,
                                                                                    "iterations": 2,
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
