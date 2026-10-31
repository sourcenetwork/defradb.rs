//! Generated from the conformance scenarios of tests/integration/explain/default/type_join_one_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.join.structure")]
fn default_explain_request_with_a_one_to_one_join() {
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
                                            OnlyEmail: contact {
                                                email
                                            }
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
                    m::explain_target("typeIndexJoin", false, 0, json!({
                        "direction": "primary",
                        "joinType": "typeJoinOne",
                        "rootName": "author",
                        "subTypeName": "contact",
                    })),
                    m::explain_target("root", true, 0, json!({
                        "scanNode": {
                            "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                            "collectionName": "Author",
                            "filter": null,
                            "prefixes": ["/3"],
                        },
                    })),
                    m::explain_target("subType", true, 0, json!({
                        "selectTopNode": {
                            "selectNode": {
                                "docID": null,
                                "filter": null,
                                "scanNode": {
                                    "collectionID": "bafyreie2qrsugrpukipgyuxhdtneyjf4tstssauisjvjfqhps4trc4c2py",
                                    "collectionName": "AuthorContact",
                                    "filter": null,
                                    "prefixes": ["/4"],
                                },
                            },
                        },
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
#[mantra_macros::req_test("go.explain.default.join.structure")]
fn default_explain_request_with_two_level_deep_nested_joins() {
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
                                            contact {
                                                email
                                                address {
                                                    city
                                                }
                                            }
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
                                                    "selectTopNode": {
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
                                        },
                                    },
                                },
                            ],
                        },
                    })),
                    m::explain_target("typeIndexJoin", false, 0, json!({
                        "direction": "primary",
                        "joinType": "typeJoinOne",
                        "rootName": "author",
                        "subTypeName": "contact",
                    })),
                    m::explain_target("root", true, 0, json!({
                        "scanNode": {
                            "collectionID": "bafyreid73sgzodav5hxhrsypjapj6r2uzo7mhm3vqykjhfehj7i5hhksuu",
                            "collectionName": "Author",
                            "filter": null,
                            "prefixes": ["/3"],
                        },
                    })),
                    m::explain_target("typeIndexJoin", false, 1, json!({
                        "direction": "primary",
                        "joinType": "typeJoinOne",
                        "rootName": "contact",
                        "subTypeName": "address",
                    })),
                    m::explain_target("root", true, 1, json!({
                        "scanNode": {
                            "collectionID": "bafyreie2qrsugrpukipgyuxhdtneyjf4tstssauisjvjfqhps4trc4c2py",
                            "collectionName": "AuthorContact",
                            "filter": null,
                            "prefixes": ["/4"],
                        },
                    })),
                    m::explain_target("subType", true, 1, json!({
                        "selectTopNode": {
                            "selectNode": {
                                "docID": null,
                                "filter": null,
                                "scanNode": {
                                    "collectionID": "bafyreiatnwrljteqnieatpmki2m2d5jjr33np4cumtbzzl2ywfs34yeg3i",
                                    "collectionName": "ContactAddress",
                                    "filter": null,
                                    "prefixes": ["/5"],
                                },
                            },
                        },
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
