//! Generated from the conformance scenarios of tests/integration/collection_version/input_type_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.introspection.group_order_inputs")]
fn input_type_of_order_field_where_collection_has_many_relation_type() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type user {
                        age: Int
                        name: String
                        points: Float
                        verified: Boolean
                        group: group
                    }

                    type group {
                        members: [user]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type (name: "group") {
                            name
                            fields {
                                name
                                args {
                                    name
                                    type {
                                        name
                                        ofType {
                                            name
                                            kind
                                        }
                                        inputFields {
                                            name
                                            type {
                                                name
                                                ofType {
                                                    name
                                                    kind
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "fields": m::contains(json!([
                            {
                                "args": [
                                    {
                                        "name": "docID",
                                        "type": {
                                            "inputFields": null,
                                            "name": null,
                                            "ofType": { "kind": "NON_NULL", "name": null },
                                        },
                                    },
                                    {
                                        "name": "filter",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "_alias",
                                                    "type": { "name": "JSON", "ofType": null },
                                                },
                                                {
                                                    "name": "_and",
                                                    "type": {
                                                        "name": null,
                                                        "ofType": {
                                                            "kind": "NON_NULL",
                                                            "name": null,
                                                        },
                                                    },
                                                },
                                                {
                                                    "name": "_docID",
                                                    "type": {
                                                        "name": "IDOperatorBlock",
                                                        "ofType": null,
                                                    },
                                                },
                                                {
                                                    "name": "_not",
                                                    "type": {
                                                        "name": "groupFilterArg",
                                                        "ofType": null,
                                                    },
                                                },
                                                {
                                                    "name": "_or",
                                                    "type": {
                                                        "name": null,
                                                        "ofType": {
                                                            "kind": "NON_NULL",
                                                            "name": null,
                                                        },
                                                    },
                                                },
                                                {
                                                    "name": "members",
                                                    "type": {
                                                        "name": "userFilterArg",
                                                        "ofType": null,
                                                    },
                                                },
                                            ],
                                            "name": "groupFilterArg",
                                            "ofType": null,
                                        },
                                    },
                                    {
                                        "name": "groupBy",
                                        "type": {
                                            "inputFields": null,
                                            "name": null,
                                            "ofType": { "kind": "NON_NULL", "name": null },
                                        },
                                    },
                                    {
                                        "name": "limit",
                                        "type": {
                                            "inputFields": null,
                                            "name": "Int",
                                            "ofType": null,
                                        },
                                    },
                                    {
                                        "name": "offset",
                                        "type": {
                                            "inputFields": null,
                                            "name": "Int",
                                            "ofType": null,
                                        },
                                    },
                                    {
                                        "name": "order",
                                        "type": {
                                            "inputFields": null,
                                            "name": null,
                                            "ofType": {
                                                "kind": "INPUT_OBJECT",
                                                "name": "groupOrderArg",
                                            },
                                        },
                                    },
                                ],
                                "name": "GROUP",
                            },
                        ])),
                        "name": "group",
                    })),
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
#[mantra_macros::req_test("go.collection_version.introspection.group_order_inputs")]
fn input_type_of_order_field_where_collection_has_relation_type() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type book {
                        name: String
                        rating: Float
                        author: author
                    }

                    type author {
                        name: String
                        age: Int
                        verified: Boolean
                        wrote: book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type (name: "author") {
                            name
                            fields {
                                name
                                args {
                                    name
                                    type {
                                        name
                                        ofType {
                                            name
                                            kind
                                        }
                                        inputFields {
                                            name
                                            type {
                                                name
                                                ofType {
                                                    name
                                                    kind
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "fields": m::contains(json!([
                            {
                                "args": [
                                    {
                                        "name": "docID",
                                        "type": {
                                            "inputFields": null,
                                            "name": null,
                                            "ofType": { "kind": "NON_NULL", "name": null },
                                        },
                                    },
                                    {
                                        "name": "filter",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "_alias",
                                                    "type": { "name": "JSON", "ofType": null },
                                                },
                                                {
                                                    "name": "_and",
                                                    "type": {
                                                        "name": null,
                                                        "ofType": {
                                                            "kind": "NON_NULL",
                                                            "name": null,
                                                        },
                                                    },
                                                },
                                                {
                                                    "name": "_docID",
                                                    "type": {
                                                        "name": "IDOperatorBlock",
                                                        "ofType": null,
                                                    },
                                                },
                                                {
                                                    "name": "_not",
                                                    "type": {
                                                        "name": "authorFilterArg",
                                                        "ofType": null,
                                                    },
                                                },
                                                {
                                                    "name": "_or",
                                                    "type": {
                                                        "name": null,
                                                        "ofType": {
                                                            "kind": "NON_NULL",
                                                            "name": null,
                                                        },
                                                    },
                                                },
                                                {
                                                    "name": "_wroteID",
                                                    "type": {
                                                        "name": "IDOperatorBlock",
                                                        "ofType": null,
                                                    },
                                                },
                                                {
                                                    "name": "age",
                                                    "type": {
                                                        "name": "IntOperatorBlock",
                                                        "ofType": null,
                                                    },
                                                },
                                                {
                                                    "name": "name",
                                                    "type": {
                                                        "name": "StringOperatorBlock",
                                                        "ofType": null,
                                                    },
                                                },
                                                {
                                                    "name": "verified",
                                                    "type": {
                                                        "name": "BooleanOperatorBlock",
                                                        "ofType": null,
                                                    },
                                                },
                                                {
                                                    "name": "wrote",
                                                    "type": {
                                                        "name": "bookFilterArg",
                                                        "ofType": null,
                                                    },
                                                },
                                            ],
                                            "name": "authorFilterArg",
                                            "ofType": null,
                                        },
                                    },
                                    {
                                        "name": "groupBy",
                                        "type": {
                                            "inputFields": null,
                                            "name": null,
                                            "ofType": { "kind": "NON_NULL", "name": null },
                                        },
                                    },
                                    {
                                        "name": "limit",
                                        "type": {
                                            "inputFields": null,
                                            "name": "Int",
                                            "ofType": null,
                                        },
                                    },
                                    {
                                        "name": "offset",
                                        "type": {
                                            "inputFields": null,
                                            "name": "Int",
                                            "ofType": null,
                                        },
                                    },
                                    {
                                        "name": "order",
                                        "type": {
                                            "inputFields": null,
                                            "name": null,
                                            "ofType": {
                                                "kind": "INPUT_OBJECT",
                                                "name": "authorOrderArg",
                                            },
                                        },
                                    },
                                ],
                                "name": "GROUP",
                            },
                        ])),
                        "name": "author",
                    })),
                }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
