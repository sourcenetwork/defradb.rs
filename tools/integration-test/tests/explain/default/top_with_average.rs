//! Generated from the conformance scenarios of tests/integration/explain/default/top_with_average_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.aggregate.top_level")]
fn default_explain_top_level_average_request_with_filter() {
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
                                        AVG(
                                            Author: {
                                                field: age,
                                                filter: {
                                                    age: {
                                                        _gt: 26
                                                    }
                                                }
                                            }
                                        )
                                    }
                "#.into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "topLevelNode": [
                                        {
                                            "selectTopNode": { "selectNode": { "scanNode": {} } },
                                        },
                                        { "sumNode": {} },
                                        { "countNode": {} },
                                        { "averageNode": {} },
                                    ],
                                },
                            ],
                        },
                    })),
                    m::explain_target("scanNode", true, 0, json!({
                        "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                        "collectionName": "Author",
                        "filter": { "age": { "_gt": 26, "_neq": null } },
                        "prefixes": ["/3"],
                    })),
                    m::explain_target("sumNode", true, 0, json!({
                        "sources": [
                            {
                                "childFieldName": "age",
                                "fieldName": "Author",
                                "filter": { "age": { "_gt": 26, "_neq": null } },
                            },
                        ],
                    })),
                    m::explain_target("countNode", true, 0, json!({
                        "sources": [
                            {
                                "fieldName": "Author",
                                "filter": { "age": { "_gt": 26, "_neq": null } },
                            },
                        ],
                    })),
                    m::explain_target("averageNode", true, 0, json!({})),
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
#[mantra_macros::req_test("go.explain.default.aggregate.top_level")]
fn default_explain_top_level_average_request() {
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
                                        AVG(
                                            Author: {
                                                field: age
                                            }
                                        )
                                    }
                "#.into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "topLevelNode": [
                                        {
                                            "selectTopNode": { "selectNode": { "scanNode": {} } },
                                        },
                                        { "sumNode": {} },
                                        { "countNode": {} },
                                        { "averageNode": {} },
                                    ],
                                },
                            ],
                        },
                    })),
                    m::explain_target("scanNode", true, 0, json!({
                        "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                        "collectionName": "Author",
                        "filter": { "age": { "_neq": null } },
                        "prefixes": ["/3"],
                    })),
                    m::explain_target("sumNode", true, 0, json!({
                        "sources": [
                            {
                                "childFieldName": "age",
                                "fieldName": "Author",
                                "filter": { "age": { "_neq": null } },
                            },
                        ],
                    })),
                    m::explain_target("countNode", true, 0, json!({
                        "sources": [
                            {
                                "fieldName": "Author",
                                "filter": { "age": { "_neq": null } },
                            },
                        ],
                    })),
                    m::explain_target("averageNode", true, 0, json!({})),
                ]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
