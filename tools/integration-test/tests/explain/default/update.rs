//! Generated from the conformance scenarios of tests/integration/explain/default/update_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.mutation.update")]
fn default_explain_mutation_request_with_update_using_boolean_filter() {
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
                    mutation @explain {
                                        update_Author(
                                            filter: {
                                                verified: {
                                                    _eq: true
                                                }
                                            },
                                            input: {age: 59}
                                        ) {
                                            _docID
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
                                            "updateNode": {
                                                "selectTopNode": { "selectNode": { "scanNode": {} } },
                                            },
                                        },
                                    },
                                },
                            ],
                        },
                    })),
                    m::explain_target("updateNode", false, 0, json!({
                        "docID": null,
                        "filter": { "verified": { "_eq": true } },
                        "input": { "age": 59 },
                    })),
                    m::explain_target("scanNode", true, 0, json!({
                        "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                        "collectionName": "Author",
                        "filter": { "verified": { "_eq": true } },
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
#[mantra_macros::req_test("go.explain.default.mutation.update")]
fn default_explain_mutation_request_with_update_using_id() {
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
                    mutation @explain {
                                        update_Author(
                                            docID: "bae-bfbfc89c-0d63-5ea4-81a3-3ebd295be67f",
                                            input: {age: 59}
                                        ) {
                                            _docID
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
                                            "updateNode": {
                                                "selectTopNode": { "selectNode": { "scanNode": {} } },
                                            },
                                        },
                                    },
                                },
                            ],
                        },
                    })),
                    m::explain_target("updateNode", false, 0, json!({
                        "docID": ["bae-bfbfc89c-0d63-5ea4-81a3-3ebd295be67f"],
                        "filter": null,
                        "input": { "age": 59 },
                    })),
                    m::explain_target("scanNode", true, 0, json!({
                        "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                        "collectionName": "Author",
                        "filter": null,
                        "prefixes": [],
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
#[mantra_macros::req_test("go.explain.default.mutation.update")]
fn default_explain_mutation_request_with_update_using_ids_and_filter() {
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
                    mutation @explain {
                                        update_Author(
                                            filter: {
                                                verified: {
                                                    _eq: true
                                                }
                                            },
                                            docID: [
                                                "bae-bfbfc89c-0d63-5ea4-81a3-3ebd295be67f",
                                                "bae-079d0bd8-4b1b-5f5f-bd95-4d915c277f9d"
                                            ],
                                            input: {age: 59}
                                        ) {
                                            _docID
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
                                            "updateNode": {
                                                "selectTopNode": { "selectNode": { "scanNode": {} } },
                                            },
                                        },
                                    },
                                },
                            ],
                        },
                    })),
                    m::explain_target("updateNode", false, 0, json!({
                        "docID": [
                            "bae-bfbfc89c-0d63-5ea4-81a3-3ebd295be67f",
                            "bae-079d0bd8-4b1b-5f5f-bd95-4d915c277f9d",
                        ],
                        "filter": { "verified": { "_eq": true } },
                        "input": { "age": 59 },
                    })),
                    m::explain_target("scanNode", true, 0, json!({
                        "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                        "collectionName": "Author",
                        "filter": { "verified": { "_eq": true } },
                        "prefixes": [],
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
#[mantra_macros::req_test("go.explain.default.mutation.update")]
fn default_explain_mutation_request_with_update_using_ids() {
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
                    mutation @explain {
                                        update_Author(
                                            docID: [
                                                "bae-bfbfc89c-0d63-5ea4-81a3-3ebd295be67f",
                                                "bae-079d0bd8-4b1b-5f5f-bd95-4d915c277f9d"
                                            ],
                                            input: {age: 59}
                                        ) {
                                            _docID
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
                                            "updateNode": {
                                                "selectTopNode": { "selectNode": { "scanNode": {} } },
                                            },
                                        },
                                    },
                                },
                            ],
                        },
                    })),
                    m::explain_target("updateNode", false, 0, json!({
                        "docID": [
                            "bae-bfbfc89c-0d63-5ea4-81a3-3ebd295be67f",
                            "bae-079d0bd8-4b1b-5f5f-bd95-4d915c277f9d",
                        ],
                        "filter": null,
                        "input": { "age": 59 },
                    })),
                    m::explain_target("scanNode", true, 0, json!({
                        "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                        "collectionName": "Author",
                        "filter": null,
                        "prefixes": [],
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
