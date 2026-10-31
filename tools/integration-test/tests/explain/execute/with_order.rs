//! Generated from the conformance scenarios of tests/integration/explain/execute/with_order_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.errors")]
fn execute_explain_request_where_parent_field_is_ordered_by_child_field() {
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
                                        Author(
                                            order: {
                                                articles: {pages: ASC}
                                            }
                                        ) {
                                            name
                                            articles {
                                                pages
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(
                    r#"Argument "order" has invalid value {articles: {pages: ASC}}.
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
#[mantra_macros::req_test("go.explain.execute.shaping")]
fn execute_explain_request_with_multi_order_fields_on_parent() {
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
                doc_map: Some(json!({ "name": "Another64YearOld", "age": 64 })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 48 })),
                common: Common {
                    capture: captures([("doc2_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Another48YearOld", "age": 48 })),
                common: Common {
                    capture: captures([("doc2_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        Author(order: [{age: ASC}, {name: DESC}]) {
                                            name
                                            age
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
                                    "orderNode": {
                                        "iterations": 5,
                                        "selectNode": {
                                            "filterMatches": 4,
                                            "iterations": 5,
                                            "scanNode": {
                                                "docFetches": 4,
                                                "fieldFetches": 8,
                                                "indexFetches": 0,
                                                "iterations": 5,
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
#[mantra_macros::req_test("go.explain.execute.shaping")]
fn execute_explain_request_with_order_field_on_both_parent_and_child() {
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
                                        Author(order: {age: ASC}) {
                                            name
                                            age
                                            articles(order: {pages: DESC}) {
                                                pages
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
                                    "orderNode": {
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
                                                            "orderNode": {
                                                                "iterations": 5,
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
#[mantra_macros::req_test("go.explain.execute.shaping")]
fn execute_explain_request_with_order_field_on_child() {
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
                                            articles(order: {pages: DESC}) {
                                                pages
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
                                                        "orderNode": {
                                                            "iterations": 5,
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
#[mantra_macros::req_test("go.explain.execute.shaping")]
fn execute_explain_request_with_order_field_on_parent() {
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
                                        Author(order: {age: ASC}) {
                                            name
                                            age
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
                                    "orderNode": {
                                        "iterations": 3,
                                        "selectNode": {
                                            "filterMatches": 2,
                                            "iterations": 3,
                                            "scanNode": {
                                                "docFetches": 2,
                                                "fieldFetches": 8,
                                                "indexFetches": 0,
                                                "iterations": 3,
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
#[mantra_macros::req_test("go.explain.execute.shaping")]
fn execute_explain_request_with_subquery_order_by_nested_relation_field_asc() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        title: String
                        author: Author
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        establishedYear: Int
                        book: Book @primary
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2020" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2010" })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_0"),
                    "establishedYear": 2020,
                    "name": "Publisher2020",
                })),
                common: Common { capture: captures([("doc2_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_1"),
                    "establishedYear": 2010,
                    "name": "Publisher2010",
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
                                            published(order: {publisher: {establishedYear: ASC}}, limit: 2) {
                                                title
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
                                        "filterMatches": 1,
                                        "iterations": 2,
                                        "typeIndexJoin": {
                                            "iterations": 2,
                                            "typeJoinMany": {
                                                "root": {
                                                    "scanNode": {
                                                        "docFetches": 1,
                                                        "fieldFetches": 1,
                                                        "indexFetches": 0,
                                                        "iterations": 2,
                                                    },
                                                },
                                                "subType": {
                                                    "selectTopNode": {
                                                        "limitNode": {
                                                            "iterations": 3,
                                                            "orderNode": {
                                                                "iterations": 2,
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
#[mantra_macros::req_test("go.explain.execute.shaping")]
fn execute_explain_request_with_subquery_order_by_nested_relation_field() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        title: String
                        author: Author
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        establishedYear: Int
                        book: Book @primary
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2020" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2010" })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_0"),
                    "establishedYear": 2020,
                    "name": "Publisher2020",
                })),
                common: Common { capture: captures([("doc2_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_1"),
                    "establishedYear": 2010,
                    "name": "Publisher2010",
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
                                            published(order: {publisher: {establishedYear: DESC}}, limit: 2) {
                                                title
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
                                        "filterMatches": 1,
                                        "iterations": 2,
                                        "typeIndexJoin": {
                                            "iterations": 2,
                                            "typeJoinMany": {
                                                "root": {
                                                    "scanNode": {
                                                        "docFetches": 1,
                                                        "fieldFetches": 1,
                                                        "indexFetches": 0,
                                                        "iterations": 2,
                                                    },
                                                },
                                                "subType": {
                                                    "selectTopNode": {
                                                        "limitNode": {
                                                            "iterations": 3,
                                                            "orderNode": {
                                                                "iterations": 2,
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
