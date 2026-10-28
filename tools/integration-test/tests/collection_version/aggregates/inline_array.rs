//! Generated from the conformance scenarios of tests/integration/collection_version/aggregates/inline_array_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_inline_array")]
fn collection_version_aggregate_inline_array_adds_users_average() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        favouriteIntegers: [Int!]
                    }
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
                                                    "name": "field",
                                                    "type": { "name": null },
                                                },
                                                {
                                                    "name": "filter",
                                                    "type": { "name": "UsersFilterArg" },
                                                },
                                                {
                                                    "name": "limit",
                                                    "type": { "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "name": "Int" },
                                                },
                                                {
                                                    "name": "order",
                                                    "type": { "name": null },
                                                },
                                            ],
                                            "name": "Users__NumericSelector",
                                        },
                                    },
                                    {
                                        "name": "favouriteIntegers",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": { "name": "NotNullIntFilterArg" },
                                                },
                                                {
                                                    "name": "limit",
                                                    "type": { "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "name": "Int" },
                                                },
                                                {
                                                    "name": "order",
                                                    "type": { "name": "Ordering" },
                                                },
                                            ],
                                            "name": "Users__favouriteIntegers__NumericSelector",
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
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_inline_array")]
fn collection_version_aggregate_inline_array_adds_users_boolean_count_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Favourites: [Boolean!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query IntrospectionQuery {
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
                    }
                "#.into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "fields": m::contains(json!([
                            {
                                "args": [
                                    {
                                        "name": "Favourites",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_eq",
                                                                "type": { "name": "Boolean" },
                                                            },
                                                            {
                                                                "name": "_in",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_neq",
                                                                "type": { "name": "Boolean" },
                                                            },
                                                            {
                                                                "name": "_nin",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
                                                            },
                                                        ],
                                                        "name": "NotNullBooleanFilterArg",
                                                    },
                                                },
                                                {
                                                    "name": "limit",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                            ],
                                            "name": "Users__Favourites__CountSelector",
                                        },
                                    },
                                    {
                                        "name": "GROUP",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "Favourites",
                                                                "type": {
                                                                    "name": "NotNullBooleanListOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_alias",
                                                                "type": { "name": "JSON" },
                                                            },
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_docID",
                                                                "type": {
                                                                    "name": "IDOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_not",
                                                                "type": {
                                                                    "name": "UsersFilterArg",
                                                                },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
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
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
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
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_inline_array")]
fn collection_version_aggregate_inline_array_adds_users_count() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        favouriteIntegers: [Int!]
                    }
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
                                    {
                                        "name": "favouriteIntegers",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": { "name": "NotNullIntFilterArg" },
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
                                            "name": "Users__favouriteIntegers__CountSelector",
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
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_inline_array")]
fn collection_version_aggregate_inline_array_adds_users_float_count_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Favourites: [Float!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query IntrospectionQuery {
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
                    }
                "#.into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "fields": m::contains(json!([
                            {
                                "args": [
                                    {
                                        "name": "Favourites",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_eq",
                                                                "type": { "name": "Float64" },
                                                            },
                                                            {
                                                                "name": "_geq",
                                                                "type": { "name": "Float64" },
                                                            },
                                                            {
                                                                "name": "_gt",
                                                                "type": { "name": "Float64" },
                                                            },
                                                            {
                                                                "name": "_in",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_leq",
                                                                "type": { "name": "Float64" },
                                                            },
                                                            {
                                                                "name": "_lt",
                                                                "type": { "name": "Float64" },
                                                            },
                                                            {
                                                                "name": "_neq",
                                                                "type": { "name": "Float64" },
                                                            },
                                                            {
                                                                "name": "_nin",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
                                                            },
                                                        ],
                                                        "name": "NotNullFloat64FilterArg",
                                                    },
                                                },
                                                {
                                                    "name": "limit",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                            ],
                                            "name": "Users__Favourites__CountSelector",
                                        },
                                    },
                                    {
                                        "name": "GROUP",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "Favourites",
                                                                "type": {
                                                                    "name": "NotNullFloat64ListOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_alias",
                                                                "type": { "name": "JSON" },
                                                            },
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_docID",
                                                                "type": {
                                                                    "name": "IDOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_not",
                                                                "type": {
                                                                    "name": "UsersFilterArg",
                                                                },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
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
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
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
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_inline_array")]
fn collection_version_aggregate_inline_array_adds_users_integer_count_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Favourites: [Int!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query IntrospectionQuery {
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
                    }
                "#.into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "fields": m::contains(json!([
                            {
                                "args": [
                                    {
                                        "name": "Favourites",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_eq",
                                                                "type": { "name": "Int" },
                                                            },
                                                            {
                                                                "name": "_geq",
                                                                "type": { "name": "Int" },
                                                            },
                                                            {
                                                                "name": "_gt",
                                                                "type": { "name": "Int" },
                                                            },
                                                            {
                                                                "name": "_in",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_leq",
                                                                "type": { "name": "Int" },
                                                            },
                                                            {
                                                                "name": "_lt",
                                                                "type": { "name": "Int" },
                                                            },
                                                            {
                                                                "name": "_neq",
                                                                "type": { "name": "Int" },
                                                            },
                                                            {
                                                                "name": "_nin",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
                                                            },
                                                        ],
                                                        "name": "NotNullIntFilterArg",
                                                    },
                                                },
                                                {
                                                    "name": "limit",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                            ],
                                            "name": "Users__Favourites__CountSelector",
                                        },
                                    },
                                    {
                                        "name": "GROUP",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "Favourites",
                                                                "type": {
                                                                    "name": "NotNullIntListOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_alias",
                                                                "type": { "name": "JSON" },
                                                            },
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_docID",
                                                                "type": {
                                                                    "name": "IDOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_not",
                                                                "type": {
                                                                    "name": "UsersFilterArg",
                                                                },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
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
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
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
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_inline_array")]
fn collection_version_aggregate_inline_array_adds_users_nillable_boolean_count_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Favourites: [Boolean]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query IntrospectionQuery {
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
                    }
                "#.into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "fields": m::contains(json!([
                            {
                                "args": [
                                    {
                                        "name": "Favourites",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_eq",
                                                                "type": { "name": "Boolean" },
                                                            },
                                                            {
                                                                "name": "_in",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_neq",
                                                                "type": { "name": "Boolean" },
                                                            },
                                                            {
                                                                "name": "_nin",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
                                                            },
                                                        ],
                                                        "name": "BooleanFilterArg",
                                                    },
                                                },
                                                {
                                                    "name": "limit",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                            ],
                                            "name": "Users__Favourites__CountSelector",
                                        },
                                    },
                                    {
                                        "name": "GROUP",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "Favourites",
                                                                "type": {
                                                                    "name": "BooleanListOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_alias",
                                                                "type": { "name": "JSON" },
                                                            },
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_docID",
                                                                "type": {
                                                                    "name": "IDOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_not",
                                                                "type": {
                                                                    "name": "UsersFilterArg",
                                                                },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
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
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
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
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_inline_array")]
fn collection_version_aggregate_inline_array_adds_users_nillable_float_count_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Favourites: [Float]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query IntrospectionQuery {
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
                    }
                "#.into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "fields": m::contains(json!([
                            {
                                "args": [
                                    {
                                        "name": "Favourites",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_eq",
                                                                "type": { "name": "Float64" },
                                                            },
                                                            {
                                                                "name": "_geq",
                                                                "type": { "name": "Float64" },
                                                            },
                                                            {
                                                                "name": "_gt",
                                                                "type": { "name": "Float64" },
                                                            },
                                                            {
                                                                "name": "_in",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_leq",
                                                                "type": { "name": "Float64" },
                                                            },
                                                            {
                                                                "name": "_lt",
                                                                "type": { "name": "Float64" },
                                                            },
                                                            {
                                                                "name": "_neq",
                                                                "type": { "name": "Float64" },
                                                            },
                                                            {
                                                                "name": "_nin",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
                                                            },
                                                        ],
                                                        "name": "Float64FilterArg",
                                                    },
                                                },
                                                {
                                                    "name": "limit",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                            ],
                                            "name": "Users__Favourites__CountSelector",
                                        },
                                    },
                                    {
                                        "name": "GROUP",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "Favourites",
                                                                "type": {
                                                                    "name": "Float64ListOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_alias",
                                                                "type": { "name": "JSON" },
                                                            },
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_docID",
                                                                "type": {
                                                                    "name": "IDOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_not",
                                                                "type": {
                                                                    "name": "UsersFilterArg",
                                                                },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
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
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
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
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_inline_array")]
fn collection_version_aggregate_inline_array_adds_users_nillable_integer_count_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Favourites: [Int]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query IntrospectionQuery {
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
                    }
                "#
                .into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "fields": m::contains(json!([
                            {
                                "args": [
                                    {
                                        "name": "Favourites",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_eq",
                                                                "type": { "name": "Int" },
                                                            },
                                                            {
                                                                "name": "_geq",
                                                                "type": { "name": "Int" },
                                                            },
                                                            {
                                                                "name": "_gt",
                                                                "type": { "name": "Int" },
                                                            },
                                                            {
                                                                "name": "_in",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_leq",
                                                                "type": { "name": "Int" },
                                                            },
                                                            {
                                                                "name": "_lt",
                                                                "type": { "name": "Int" },
                                                            },
                                                            {
                                                                "name": "_neq",
                                                                "type": { "name": "Int" },
                                                            },
                                                            {
                                                                "name": "_nin",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
                                                            },
                                                        ],
                                                        "name": "IntFilterArg",
                                                    },
                                                },
                                                {
                                                    "name": "limit",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                            ],
                                            "name": "Users__Favourites__CountSelector",
                                        },
                                    },
                                    {
                                        "name": "GROUP",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "Favourites",
                                                                "type": {
                                                                    "name": "IntListOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_alias",
                                                                "type": { "name": "JSON" },
                                                            },
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_docID",
                                                                "type": {
                                                                    "name": "IDOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_not",
                                                                "type": {
                                                                    "name": "UsersFilterArg",
                                                                },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
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
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
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
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_inline_array")]
fn collection_version_aggregate_inline_array_adds_users_nillable_string_count_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Favourites: [String]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query IntrospectionQuery {
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
                    }
                "#.into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "fields": m::contains(json!([
                            {
                                "args": [
                                    {
                                        "name": "Favourites",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_eq",
                                                                "type": { "name": "String" },
                                                            },
                                                            {
                                                                "name": "_ilike",
                                                                "type": { "name": "String" },
                                                            },
                                                            {
                                                                "name": "_in",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_like",
                                                                "type": { "name": "String" },
                                                            },
                                                            {
                                                                "name": "_neq",
                                                                "type": { "name": "String" },
                                                            },
                                                            {
                                                                "name": "_nilike",
                                                                "type": { "name": "String" },
                                                            },
                                                            {
                                                                "name": "_nin",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_nlike",
                                                                "type": { "name": "String" },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
                                                            },
                                                        ],
                                                        "name": "StringFilterArg",
                                                    },
                                                },
                                                {
                                                    "name": "limit",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                            ],
                                            "name": "Users__Favourites__CountSelector",
                                        },
                                    },
                                    {
                                        "name": "GROUP",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "Favourites",
                                                                "type": {
                                                                    "name": "StringListOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_alias",
                                                                "type": { "name": "JSON" },
                                                            },
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_docID",
                                                                "type": {
                                                                    "name": "IDOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_not",
                                                                "type": {
                                                                    "name": "UsersFilterArg",
                                                                },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
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
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
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
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_inline_array")]
fn collection_version_aggregate_inline_array_adds_users_string_count_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Favourites: [String!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query IntrospectionQuery {
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
                    }
                "#.into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "fields": m::contains(json!([
                            {
                                "args": [
                                    {
                                        "name": "Favourites",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_eq",
                                                                "type": { "name": "String" },
                                                            },
                                                            {
                                                                "name": "_ilike",
                                                                "type": { "name": "String" },
                                                            },
                                                            {
                                                                "name": "_in",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_like",
                                                                "type": { "name": "String" },
                                                            },
                                                            {
                                                                "name": "_neq",
                                                                "type": { "name": "String" },
                                                            },
                                                            {
                                                                "name": "_nilike",
                                                                "type": { "name": "String" },
                                                            },
                                                            {
                                                                "name": "_nin",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_nlike",
                                                                "type": { "name": "String" },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
                                                            },
                                                        ],
                                                        "name": "NotNullStringFilterArg",
                                                    },
                                                },
                                                {
                                                    "name": "limit",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                            ],
                                            "name": "Users__Favourites__CountSelector",
                                        },
                                    },
                                    {
                                        "name": "GROUP",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": {
                                                        "inputFields": [
                                                            {
                                                                "name": "Favourites",
                                                                "type": {
                                                                    "name": "NotNullStringListOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_alias",
                                                                "type": { "name": "JSON" },
                                                            },
                                                            {
                                                                "name": "_and",
                                                                "type": { "name": null },
                                                            },
                                                            {
                                                                "name": "_docID",
                                                                "type": {
                                                                    "name": "IDOperatorBlock",
                                                                },
                                                            },
                                                            {
                                                                "name": "_not",
                                                                "type": {
                                                                    "name": "UsersFilterArg",
                                                                },
                                                            },
                                                            {
                                                                "name": "_or",
                                                                "type": { "name": null },
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
                                                    "type": { "inputFields": null, "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "inputFields": null, "name": "Int" },
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
#[mantra_macros::req_test("go.collection_version.introspection.aggregate_inline_array")]
fn collection_version_aggregate_inline_array_adds_users_sum() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        FavouriteFloats: [Float!]
                    }
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
                                        "name": "FavouriteFloats",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "filter",
                                                    "type": { "name": "NotNullFloat64FilterArg" },
                                                },
                                                {
                                                    "name": "limit",
                                                    "type": { "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "name": "Int" },
                                                },
                                                {
                                                    "name": "order",
                                                    "type": { "name": "Ordering" },
                                                },
                                            ],
                                            "name": "Users__FavouriteFloats__NumericSelector",
                                        },
                                    },
                                    {
                                        "name": "GROUP",
                                        "type": {
                                            "inputFields": [
                                                {
                                                    "name": "field",
                                                    "type": { "name": null },
                                                },
                                                {
                                                    "name": "filter",
                                                    "type": { "name": "UsersFilterArg" },
                                                },
                                                {
                                                    "name": "limit",
                                                    "type": { "name": "Int" },
                                                },
                                                {
                                                    "name": "offset",
                                                    "type": { "name": "Int" },
                                                },
                                                {
                                                    "name": "order",
                                                    "type": { "name": null },
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
