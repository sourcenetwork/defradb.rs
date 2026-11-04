//! Generated from the conformance scenarios of tests/integration/explain/default/with_count_join_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.aggregate.shared_join")]
fn default_explain_request_one_to_many_with_count_and_child_filter_does_not_share_join_field() {
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
                                            COUNT(books: {})
                                            books(filter: {rating: {_neq: null}}){
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!(m::explain_pattern(json!({
                    "explain": {
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "countNode": {
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
#[mantra_macros::req_test("go.explain.default.aggregate.shared_join")]
fn default_explain_request_one_to_many_with_count_with_filter_and_child_filter_shares_join_field() {
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
                                        Author {
                                            name
                                            COUNT(books: {filter: {rating: {_neq: null}}})
                                            books(filter: {rating: {_neq: null}}){
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::explain_pattern(json!({
                    "explain": {
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "countNode": {
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
#[mantra_macros::req_test("go.explain.default.aggregate.join")]
fn default_explain_request_with_count_on_one_to_many_joined_field_with_many_sources() {
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
                                        Author {
                                            name
                                            numberOfBooks: COUNT(
                                                books: {}
                                                articles: {}
                                            )
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "selectTopNode": {
                                        "countNode": {
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
                                },
                            ],
                        },
                    })),
                    m::explain_target("countNode", false, 0, json!({
                        "sources": [
                            { "fieldName": "books", "filter": null },
                            { "fieldName": "articles", "filter": null },
                        ],
                    })),
                    m::explain_target("typeIndexJoin", false, 0, json!({
                        "joinType": "typeJoinMany",
                        "rootName": "author",
                        "subTypeName": "books",
                    })),
                    m::explain_target("scanNode", true, 0, json!({
                        "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                        "collectionName": "Author",
                        "filter": null,
                        "prefixes": ["/3"],
                    })),
                    m::explain_target("scanNode", true, 1, json!({
                        "collectionID": "bafyreifnc6yphaqxf7x7fa3phxrsuvzqvnnjz4q7fuirhty4cnrxubp6eq",
                        "collectionName": "Book",
                        "filter": null,
                        "prefixes": ["/2"],
                    })),
                    m::explain_target("typeIndexJoin", false, 1, json!({
                        "joinType": "typeJoinMany",
                        "rootName": "author",
                        "subTypeName": "articles",
                    })),
                    m::explain_target("scanNode", true, 2, json!({
                        "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                        "collectionName": "Author",
                        "filter": null,
                        "prefixes": ["/3"],
                    })),
                    m::explain_target("scanNode", true, 3, json!({
                        "collectionID": "bafyreidpjzf5kytlap2nilnnemywjmwt74hr56e72llri2z7c6w7un7sje",
                        "collectionName": "Article",
                        "filter": null,
                        "prefixes": ["/1"],
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
#[mantra_macros::req_test("go.explain.default.aggregate.join")]
fn default_explain_request_with_count_on_one_to_many_joined_field() {
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
                                        Author {
                                            name
                                            numberOfBooks: COUNT(books: {})
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "selectTopNode": {
                                        "countNode": {
                                            "selectNode": {
                                                "typeIndexJoin": {
                                                    "root": { "scanNode": {} },
                                                    "subType": {
                                                        "selectTopNode": {
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
                    m::explain_target("countNode", false, 0, json!({ "sources": [{ "fieldName": "books", "filter": null }] })),
                    m::explain_target("typeIndexJoin", false, 0, json!({
                        "joinType": "typeJoinMany",
                        "rootName": "author",
                        "subTypeName": "books",
                    })),
                    m::explain_target("scanNode", true, 0, json!({
                        "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                        "collectionName": "Author",
                        "filter": null,
                        "prefixes": ["/3"],
                    })),
                    m::explain_target("scanNode", true, 1, json!({
                        "collectionID": "bafyreifnc6yphaqxf7x7fa3phxrsuvzqvnnjz4q7fuirhty4cnrxubp6eq",
                        "collectionName": "Book",
                        "filter": null,
                        "prefixes": ["/2"],
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
