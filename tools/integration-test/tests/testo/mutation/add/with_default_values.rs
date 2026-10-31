//! Generated from the conformance scenarios of tests/integration/mutation/add/with_default_values_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.utc_now")]
fn mutation_add_with_default_values_no_values_provided_sets_utc_now_default_value() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        created: DateTime @default(value: UTC_NOW)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({})),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            created
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "created": m::current_timestamp() }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.add.default_values")]
fn mutation_add_with_default_json_bool_value_should_be_set() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        metadata: JSON @default(value: true)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            metadata
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "metadata": true }] }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.add.default_values")]
fn mutation_add_with_default_json_deep_object_value_should_be_set() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        metadata: JSON @default(value: {one: {two: {i: 3, f: 1.2, b: true, s: "three", n: null}}})
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            metadata
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "metadata": r#"{"one":{"two":{"b":true,"f":1.2,"i":3,"n":null,"s":"three"}}}"#,
                        },
                    ],
                }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.add.default_values")]
fn mutation_add_with_default_json_float_value_should_be_set() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        metadata: JSON @default(value: 1.2)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            metadata
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "metadata": 1.2 }] }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.add.default_values")]
fn mutation_add_with_default_json_int_value_should_be_set() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        metadata: JSON @default(value: 1)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            metadata
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "metadata": 1 }] }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.add.default_values")]
fn mutation_add_with_default_json_null_value_return_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type User {
                        name: String
                        metadata: JSON @default(value: null)
                    }
                "#
            .into(),
            expected_error: Some("default value is invalid".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.add.default_values")]
fn mutation_add_with_default_json_object_values_should_be_set() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        metadata: JSON @default(value: {one: 1})
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            metadata
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "metadata": r#"{"one":1}"# }] }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.duplicate")]
fn mutation_add_with_default_value_no_value_provided_added_twice_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String @default(value: "Bob")
                        age: Int @default(value: 40)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({})),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({})),
                expected_error: Some("a document with the given ID already exists".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.add.default_values")]
fn mutation_add_with_default_value_no_value_provided_added_twice_unique_index_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String @default(value: "Bob") @index(unique: true)
                        age: Int @default(value: 40)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({})),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 50 })),
                expected_error: Some(
                    "can not index a doc's field(s) that violates unique index".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.add.default_values")]
fn mutation_add_with_default_values_nil_values_provided_sets_nil_value() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        active: Boolean @default(value: true)
                        created: DateTime @default(value: "2000-07-23T03:00:00-00:00")
                        name: String @default(value: "Bob")
                        age: Int @default(value: 40)
                        points: Float @default(value: 10)
                        points32: Float32 @default(value: 11)
                        points64: Float64 @default(value: 12)
                        metadata: JSON @default(value: "{\"one\":1}")
                        image: Blob @default(value: "ff0099")
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({
                    "active": null,
                    "age": null,
                    "created": null,
                    "image": null,
                    "metadata": null,
                    "name": null,
                    "points": null,
                    "points32": null,
                    "points64": null,
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            age
                                            active
                                            name
                                            points
                                            points32
                                            points64
                                            created
                                            metadata
                                            image
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "active": null,
                            "age": null,
                            "created": null,
                            "image": null,
                            "metadata": null,
                            "name": null,
                            "points": null,
                            "points32": null,
                            "points64": null,
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.add.default_values")]
fn mutation_add_with_default_values_no_values_provided_sets_default_value() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        active: Boolean @default(value: true)
                        created: DateTime @default(value: "2000-07-23T03:00:00-00:00")
                        name: String @default(value: "Bob")
                        age: Int @default(value: 40)
                        points: Float @default(value: 10)
                        points32: Float32 @default(value: 11)
                        points64: Float64 @default(value: 12)
                        metadata: JSON @default(value: "{\"one\":1}")
                        image: Blob @default(value: "ff0099")
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({})),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            age
                                            active
                                            name
                                            points
                                            points32
                                            points64
                                            created
                                            metadata
                                            image
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "active": true,
                            "age": 40,
                            "created": "2000-07-23T03:00:00Z",
                            "image": "ff0099",
                            "metadata": r#"{"one":1}"#,
                            "name": "Bob",
                            "points": 10.0,
                            "points32": 11.0,
                            "points64": 12.0,
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.utc_now")]
fn mutation_add_with_default_values_no_values_provided_sets_two_equal_utc_now_default_value() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        created: DateTime @default(value: UTC_NOW)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        bob: add_User(input: { name: "Bob" }) {
                                            created
                                        }

                                        alice: add_User(input: { name: "Alice" }) {
                                            created
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "alice": [{ "created": m::same("timestampMatcher") }],
                    "bob": [{ "created": m::same("timestampMatcher") }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.add.default_values")]
fn mutation_add_with_default_values_values_provided_sets_value() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        active: Boolean @default(value: true)
                        created: DateTime @default(value: "2000-07-23T03:00:00-00:00")
                        name: String @default(value: "Bob")
                        age: Int @default(value: 40)
                        points: Float @default(value: 10)
                        points32: Float @default(value: 11)
                        points64: Float @default(value: 12)
                        metadata: JSON @default(value: "{\"one\":1}")
                        image: Blob @default(value: "ff0099")
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({
                    "active": false,
                    "age": 50,
                    "created": "2024-06-18T01:00:00Z",
                    "image": "aabb33",
                    "metadata": r#"{"two":2}"#,
                    "name": "Alice",
                    "points": 5.0,
                    "points32": 6.0,
                    "points64": 7.0,
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            age
                                            active
                                            name
                                            points
                                            points32
                                            points64
                                            created
                                            metadata
                                            image
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "active": false,
                            "age": 50,
                            "created": "2024-06-18T01:00:00Z",
                            "image": "aabb33",
                            "metadata": r#"{"two":2}"#,
                            "name": "Alice",
                            "points": 5.0,
                            "points32": 6.0,
                            "points64": 7.0,
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
