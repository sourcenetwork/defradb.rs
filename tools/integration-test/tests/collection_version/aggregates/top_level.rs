//! Generated from the conformance scenarios of tests/integration/collection_version/aggregates/top_level_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_fields")]
fn collection_version_aggregate_top_level_adds_average_given_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {}
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __schema {
                            queryType {
                                fields {
                                    name
                                    args {
                                        name
                                        type {
                                            name
                                            inputFields {
                                                name
                                                type {
                                                    name
                                                    kind
                                                    ofType {
                                                        name
                                                    }
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
                    "__schema": m::object(json!({
                        "queryType": m::object(json!({
                            "fields": m::contains(json!([
                                {
                                    "args": [
                                        {
                                            "name": "Users",
                                            "type": {
                                                "inputFields": [
                                                    {
                                                        "name": "field",
                                                        "type": {
                                                            "kind": "NON_NULL",
                                                            "name": null,
                                                            "ofType": {
                                                                "name": "UsersNumericFieldsArg",
                                                            },
                                                        },
                                                    },
                                                    {
                                                        "name": "filter",
                                                        "type": {
                                                            "kind": "INPUT_OBJECT",
                                                            "name": "UsersFilterArg",
                                                            "ofType": null,
                                                        },
                                                    },
                                                    {
                                                        "name": "limit",
                                                        "type": {
                                                            "kind": "SCALAR",
                                                            "name": "Int",
                                                            "ofType": null,
                                                        },
                                                    },
                                                    {
                                                        "name": "offset",
                                                        "type": {
                                                            "kind": "SCALAR",
                                                            "name": "Int",
                                                            "ofType": null,
                                                        },
                                                    },
                                                    {
                                                        "name": "order",
                                                        "type": {
                                                            "kind": "LIST",
                                                            "name": null,
                                                            "ofType": { "name": "UsersOrderArg" },
                                                        },
                                                    },
                                                ],
                                                "name": "Users__NumericSelector",
                                            },
                                        },
                                    ],
                                    "name": "AVG",
                                },
                            ])),
                        })),
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
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_fields")]
fn collection_version_aggregate_top_level_adds_count_given_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {}
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __schema {
                            queryType {
                                fields {
                                    name
                                    args {
                                        name
                                        type {
                                            name
                                            inputFields {
                                                name
                                                type {
                                                    name
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
                    "__schema": m::object(json!({
                        "queryType": m::object(json!({
                            "fields": m::contains(json!([
                                {
                                    "args": [
                                        {
                                            "name": "Users",
                                            "type": {
                                                "inputFields": [
                                                    {
                                                        "name": "filter",
                                                        "type": { "name": "UsersFilterArg" },
                                                    },
                                                    {
                                                        "name": "groupBy",
                                                        "type": { "name": null },
                                                    },
                                                    {
                                                        "name": "limit",
                                                        "type": { "name": "Int" },
                                                    },
                                                    {
                                                        "name": "offset",
                                                        "type": { "name": "Int" },
                                                    },
                                                ],
                                                "name": "Users__CountSelector",
                                            },
                                        },
                                    ],
                                    "name": "COUNT",
                                },
                            ])),
                        })),
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
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_fields")]
fn collection_version_aggregate_top_level_adds_sum_given_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {}
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __schema {
                            queryType {
                                fields {
                                    name
                                    args {
                                        name
                                        type {
                                            name
                                            inputFields {
                                                name
                                                type {
                                                    name
                                                    kind
                                                    ofType {
                                                        name
                                                    }
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
                    "__schema": m::object(json!({
                        "queryType": m::object(json!({
                            "fields": m::contains(json!([
                                {
                                    "args": [
                                        {
                                            "name": "Users",
                                            "type": {
                                                "inputFields": [
                                                    {
                                                        "name": "field",
                                                        "type": {
                                                            "kind": "NON_NULL",
                                                            "name": null,
                                                            "ofType": {
                                                                "name": "UsersNumericFieldsArg",
                                                            },
                                                        },
                                                    },
                                                    {
                                                        "name": "filter",
                                                        "type": {
                                                            "kind": "INPUT_OBJECT",
                                                            "name": "UsersFilterArg",
                                                            "ofType": null,
                                                        },
                                                    },
                                                    {
                                                        "name": "limit",
                                                        "type": {
                                                            "kind": "SCALAR",
                                                            "name": "Int",
                                                            "ofType": null,
                                                        },
                                                    },
                                                    {
                                                        "name": "offset",
                                                        "type": {
                                                            "kind": "SCALAR",
                                                            "name": "Int",
                                                            "ofType": null,
                                                        },
                                                    },
                                                    {
                                                        "name": "order",
                                                        "type": {
                                                            "kind": "LIST",
                                                            "name": null,
                                                            "ofType": { "name": "UsersOrderArg" },
                                                        },
                                                    },
                                                ],
                                                "name": "Users__NumericSelector",
                                            },
                                        },
                                    ],
                                    "name": "SUM",
                                },
                            ])),
                        })),
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
