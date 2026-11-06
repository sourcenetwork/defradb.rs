//! Generated from the conformance scenarios of tests/integration/explain/default/type_join_with_filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.join.related_filter")]
fn default_explain_request_with_many_related_filters() {
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
                    query @explain {
                                        Author(
                                            filter: {
                                                name: {_eq: "Cornelia Funke"},
                                                articles: {name: {_eq: "To my dear readers"}},
                                                books: {name: {_eq: "Theif Lord"}}
                                            }
                                        ) {
                                            name
                                            age
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "selectTopNode": {
                                        "selectNode": {
                                            "parallelNode": [
                                                {
                                                    "typeIndexJoin": {
                                                        "root": { "scanNode": {} },
                                                        "subType": {
                                                            "selectTopNode": {
                                                                "selectNode": { "scanNode": {} },
                                                            },
                                                        },
                                                    },
                                                },
                                                {
                                                    "typeIndexJoin": {
                                                        "root": { "scanNode": {} },
                                                        "subType": {
                                                            "selectTopNode": {
                                                                "selectNode": { "scanNode": {} },
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
                    })),
                    m::explain_target("selectNode", false, 0, json!({
                        "docID": null,
                        "filter": {
                            "articles": { "name": { "_eq": "To my dear readers" } },
                            "books": { "name": { "_eq": "Theif Lord" } },
                        },
                    })),
                    m::explain_target("scanNode", true, 0, json!({
                        "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                        "collectionName": "Author",
                        "filter": { "name": { "_eq": "Cornelia Funke" } },
                        "prefixes": ["/3"],
                    })),
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
#[mantra_macros::req_test("go.explain.default.join.related_filter")]
fn default_explain_request_with_related_and_regular_filter() {
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
                    query @explain {
                                        Author(
                                            filter: {
                                                name: {_eq: "John Grisham"},
                                                books: {name: {_eq: "Painted House"}}
                                            }
                                        ) {
                                            name
                                            age
                                        }
                                    }
                "#.into(),
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
                                                    "selectTopNode": { "selectNode": { "scanNode": {} } },
                                                },
                                            },
                                        },
                                    },
                                },
                            ],
                        },
                    })),
                    m::explain_target("selectNode", false, 0, json!({
                        "docID": null,
                        "filter": { "books": { "name": { "_eq": "Painted House" } } },
                    })),
                    m::explain_target("scanNode", true, 0, json!({
                        "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                        "collectionName": "Author",
                        "filter": { "name": { "_eq": "John Grisham" } },
                        "prefixes": ["/3"],
                    })),
                ]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
