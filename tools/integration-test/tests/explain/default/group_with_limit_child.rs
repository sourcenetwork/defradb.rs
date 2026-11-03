//! Generated from the conformance scenarios of tests/integration/explain/default/group_with_limit_child_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.group.limit")]
fn default_explain_request_with_limit_and_offset_on_inner_group_selection() {
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
                                        Author(groupBy: [name]) {
                                            name
                                            GROUP(limit: 2, offset: 1) {
                                                age
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
                                "limit": { "limit": 2, "offset": 1 },
                                "orderBy": null,
                            },
                        ],
                        "groupByFields": ["name"],
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
#[mantra_macros::req_test("go.explain.default.group.limit")]
fn default_explain_request_with_limit_and_offset_on_multiple_inner_group_selections() {
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
                                        Author(groupBy: [name]) {
                                            name
                                            innerFirstGroup: GROUP(limit: 1, offset: 2) {
                                                age
                                            }
                                            innerSecondGroup: GROUP(limit: 2) {
                                                age
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
                                "limit": { "limit": 1, "offset": 2 },
                                "orderBy": null,
                            },
                            {
                                "collectionName": "Author",
                                "docID": null,
                                "filter": null,
                                "groupBy": null,
                                "limit": { "limit": 2, "offset": 0 },
                                "orderBy": null,
                            },
                        ],
                        "groupByFields": ["name"],
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
