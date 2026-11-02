//! Generated from the conformance scenarios of tests/integration/collection_version/aggregates/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_fields")]
fn collection_version_aggregate_simple_adds_users_average() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users {}".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type(name: "Users") {
                            name
                            fields {
                                name
                                args {
                                    name
                                    type {
                                        name
                                        inputFields {
                                            name
                                            type { name kind ofType { name } }
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
                                        "name": "GROUP",
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
                        "name": "Users",
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
fn collection_version_aggregate_simple_adds_users_count() {
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
                        __type (name: "Users") {
                            name
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
                "#
                .into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "fields": m::contains(json!([
                            {
                                "args": [
                                    {
                                        "name": "GROUP",
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
                                    {
                                        "name": "_version",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "limit",
                                                    "type": { "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "name": "Int" },
                                                },
                                            ],
                                            "name": "Users___version__CountSelector",
                                        },
                                    },
                                ],
                                "name": "COUNT",
                            },
                        ])),
                        "name": "Users",
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
fn collection_version_aggregate_simple_adds_users_sum() {
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
                        __type (name: "Users") {
                            name
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
                "#
                .into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "fields": m::contains(json!([
                            {
                                "args": [
                                    {
                                        "name": "docID",
                                        "type": { "inputFields": null, "name": null },
                                    },
                                    {
                                        "name": "filter",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "_alias",
                                                    "type": {
                                                        "kind": "SCALAR",
                                                        "name": "JSON",
                                                        "ofType": null,
                                                    },
                                                },
                                                {
                                                    "name": "_and",
                                                    "type": {
                                                        "kind": "LIST",
                                                        "name": null,
                                                        "ofType": { "name": null },
                                                    },
                                                },
                                                {
                                                    "name": "_docID",
                                                    "type": {
                                                        "kind": "INPUT_OBJECT",
                                                        "name": "IDOperatorBlock",
                                                        "ofType": null,
                                                    },
                                                },
                                                {
                                                    "name": "_not",
                                                    "type": {
                                                        "kind": "INPUT_OBJECT",
                                                        "name": "UsersFilterArg",
                                                        "ofType": null,
                                                    },
                                                },
                                                {
                                                    "name": "_or",
                                                    "type": {
                                                        "kind": "LIST",
                                                        "name": null,
                                                        "ofType": { "name": null },
                                                    },
                                                },
                                            ],
                                            "name": "UsersFilterArg",
                                        },
                                    },
                                    {
                                        "name": "groupBy",
                                        "type": { "inputFields": null, "name": null },
                                    },
                                    {
                                        "name": "limit",
                                        "type": { "inputFields": null, "name": "Int" },
                                    },
                                    {
                                        "name": "offset",
                                        "type": { "inputFields": null, "name": "Int" },
                                    },
                                    {
                                        "name": "order",
                                        "type": { "inputFields": null, "name": null },
                                    },
                                ],
                                "name": "GROUP",
                            },
                            {
                                "args": [
                                    {
                                        "name": "GROUP",
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
                        "name": "Users",
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
