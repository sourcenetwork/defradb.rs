//! Generated from the conformance scenarios of tests/integration/collection_version/filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.introspection.filter_inputs")]
fn filter_for_one_to_one_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        author: Author
                    }

                    type Author {
                        age: Int
                        wrote: Book @primary
                    }
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
                                            "name": "cid",
                                            "type": { "inputFields": null, "name": null },
                                        },
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
                                                        "type": { "name": "JSON", "ofType": null },
                                                    },
                                                    {
                                                        "name": "_and",
                                                        "type": {
                                                            "name": null,
                                                            "ofType": { "name": null },
                                                        },
                                                    },
                                                    {
                                                        "name": "_authorID",
                                                        "type": {
                                                            "name": "IDOperatorBlock",
                                                            "ofType": null,
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
                                                            "name": "BookFilterArg",
                                                            "ofType": null,
                                                        },
                                                    },
                                                    {
                                                        "name": "_or",
                                                        "type": {
                                                            "name": null,
                                                            "ofType": { "name": null },
                                                        },
                                                    },
                                                    {
                                                        "name": "author",
                                                        "type": {
                                                            "name": "AuthorFilterArg",
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
                                                ],
                                                "name": "BookFilterArg",
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
                                        {
                                            "name": "showDeleted",
                                            "type": { "inputFields": null, "name": "Boolean" },
                                        },
                                    ],
                                    "name": "Book",
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
#[mantra_macros::req_test("go.collection_version.introspection.filter_inputs")]
fn filter_for_simple_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
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
                                            "name": "cid",
                                            "type": { "inputFields": null, "name": null },
                                        },
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
                                                        "type": { "name": "JSON", "ofType": null },
                                                    },
                                                    {
                                                        "name": "_and",
                                                        "type": {
                                                            "name": null,
                                                            "ofType": { "name": null },
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
                                                            "name": "UsersFilterArg",
                                                            "ofType": null,
                                                        },
                                                    },
                                                    {
                                                        "name": "_or",
                                                        "type": {
                                                            "name": null,
                                                            "ofType": { "name": null },
                                                        },
                                                    },
                                                    {
                                                        "name": "name",
                                                        "type": {
                                                            "name": "StringOperatorBlock",
                                                            "ofType": null,
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
                                        {
                                            "name": "showDeleted",
                                            "type": { "inputFields": null, "name": "Boolean" },
                                        },
                                    ],
                                    "name": "Users",
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
#[mantra_macros::req_test("go.collection_version.introspection.filter_inputs")]
fn collection_version_filter_inputs_with_json_field_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        custom: JSON
                    }
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
                                            "name": "cid",
                                            "type": { "inputFields": null, "name": null },
                                        },
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
                                                        "type": { "name": "JSON", "ofType": null },
                                                    },
                                                    {
                                                        "name": "_and",
                                                        "type": {
                                                            "name": null,
                                                            "ofType": { "name": null },
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
                                                            "name": "UsersFilterArg",
                                                            "ofType": null,
                                                        },
                                                    },
                                                    {
                                                        "name": "_or",
                                                        "type": {
                                                            "name": null,
                                                            "ofType": { "name": null },
                                                        },
                                                    },
                                                    {
                                                        "name": "custom",
                                                        "type": { "name": "JSON", "ofType": null },
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
                                        {
                                            "name": "showDeleted",
                                            "type": { "inputFields": null, "name": "Boolean" },
                                        },
                                    ],
                                    "name": "Users",
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
#[mantra_macros::req_test("go.collection_version.introspection.filter_inputs")]
fn collection_version_filter_inputs_with_non_nillable_json_field_uses_json_scalar() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { metadata: JSON! }".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type(name: "UsersFilterArg") {
                            inputFields {
                                name
                                type { name }
                            }
                        }
                    }
                "#.into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "inputFields": m::contains(json!([{ "name": "metadata", "type": { "name": "JSON" } }])),
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
#[mantra_macros::req_test("go.collection_version.introspection.filter_inputs")]
fn collection_version_filter_inputs_with_non_nillable_string_field_uses_string_operator_block() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String! }".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type(name: "UsersFilterArg") {
                            inputFields {
                                name
                                type { name }
                            }
                        }
                    }
                "#.into(),
                expected_data: json!(m::object(json!({
                    "__type": m::object(json!({
                        "inputFields": m::contains(json!([{ "name": "name", "type": { "name": "StringOperatorBlock" } }])),
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
