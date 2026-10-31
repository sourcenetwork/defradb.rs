//! Generated from the conformance scenarios of tests/integration/explain/default/group_with_doc_id_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.group.selection")]
fn default_explain_request_with_doc_i_ds_and_filter_on_parent_group_by() {
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
                                            groupBy: [age],
                                            filter: {age: {_eq: 20}},
                                            docID: [
                                                "bae-6a4c5bc5-b044-5a03-a868-8260af6f2254",
                                                "bae-4ea9d148-13f3-5a48-a0ef-9ffd344caeed"
                                            ]
                                        ) {
                                            age
                                            GROUP {
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "selectTopNode": { "groupNode": { "selectNode": { "scanNode": {} } } },
                                },
                            ],
                        },
                    })),
                    m::explain_target("groupNode", false, 0, json!({
                        "childSelects": [
                            {
                                "collectionName": "Author",
                                "docID": null,
                                "filter": null,
                                "groupBy": null,
                                "limit": null,
                                "orderBy": null,
                            },
                        ],
                        "groupByFields": ["age"],
                    })),
                    m::explain_target("scanNode", true, 0, json!({
                        "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                        "collectionName": "Author",
                        "filter": { "age": { "_eq": 20 } },
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
#[mantra_macros::req_test("go.explain.default.group.selection")]
fn default_explain_request_with_doc_id_on_parent_group_by() {
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
                                            groupBy: [age],
                                            docID: "bae-6a4c5bc5-b044-5a03-a868-8260af6f2254"
                                        ) {
                                            age
                                            GROUP {
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "selectTopNode": { "groupNode": { "selectNode": { "scanNode": {} } } },
                                },
                            ],
                        },
                    })),
                    m::explain_target("groupNode", false, 0, json!({
                        "childSelects": [
                            {
                                "collectionName": "Author",
                                "docID": null,
                                "filter": null,
                                "groupBy": null,
                                "limit": null,
                                "orderBy": null,
                            },
                        ],
                        "groupByFields": ["age"],
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
