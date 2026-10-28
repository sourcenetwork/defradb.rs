//! Generated from the conformance scenarios of tests/integration/explain/default/group_with_limit_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.group.limit")]
fn default_explain_request_with_limit_and_offset_on_parent_group_by() {
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
                                        Author(
                                            groupBy: [name],
                                            limit: 1,
                                            offset: 1
                                        ) {
                                            name
                                            GROUP {
                                                age
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "selectTopNode": {
                                        "limitNode": {
                                            "groupNode": { "selectNode": { "scanNode": {} } },
                                        },
                                    },
                                },
                            ],
                        },
                    })),
                    m::explain_target(
                        "groupNode",
                        false,
                        0,
                        json!({
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
                            "groupByFields": ["name"],
                        })
                    ),
                    m::explain_target("limitNode", false, 0, json!({ "limit": 1, "offset": 1 })),
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
fn default_explain_request_with_limit_on_parent_group_by_and_inner_group_selection() {
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
                                        Author(
                                            groupBy: [name],
                                            limit: 1
                                        ) {
                                            name
                                            GROUP(limit: 2) {
                                                age
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [
                                {
                                    "selectTopNode": {
                                        "limitNode": {
                                            "groupNode": { "selectNode": { "scanNode": {} } },
                                        },
                                    },
                                },
                            ],
                        },
                    })),
                    m::explain_target(
                        "groupNode",
                        false,
                        0,
                        json!({
                            "childSelects": [
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
                        })
                    ),
                    m::explain_target("limitNode", false, 0, json!({ "limit": 1, "offset": 0 })),
                ]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
