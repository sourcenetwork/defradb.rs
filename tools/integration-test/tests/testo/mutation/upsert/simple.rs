//! Generated from the conformance scenarios of tests/integration/mutation/upsert/simple_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.upsert.apply")]
fn mutation_upsert_simple_with_filter_match_and_version_updates_doc() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Alice", "age": 40 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 30 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        upsert_Users(
                                            filter: {name: {_eq: "Bob"}},
                                            add: {name: "Bob", age: 40},
                                            update: {age: 40}
                                        ) {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "upsert_Users": [{ "age": 40, "name": "Bob" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "age": 40, "name": "Bob" }, { "age": 40, "name": "Alice" }],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.mutation.upsert.errors")]
fn mutation_upsert_simple_with_filter_match_multiple_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 30 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Alice", "age": 40 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        upsert_Users(
                                            filter: {},
                                            add: {name: "Alice", age: 40},
                                            update: {age: 50}
                                        ) {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_error: Some("cannot upsert multiple matching documents".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.upsert.apply")]
fn mutation_upsert_simple_with_filter_match_on_same_field_updates_doc() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Alice", "age": 40 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 30 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        upsert_Users(
                                            filter: {name: {_eq: "Bob"}},
                                            add: {name: "Bob", age: 40},
                                            update: {name: "John"}
                                        ) {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "upsert_Users": [{ "age": 30, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "age": 30, "name": "John" }, { "age": 40, "name": "Alice" }],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.mutation.upsert.apply")]
fn mutation_upsert_simple_with_filter_match_updates_doc() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Alice", "age": 40 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 30 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        upsert_Users(
                                            filter: {name: {_eq: "Bob"}},
                                            add: {name: "Bob", age: 40},
                                            update: {age: 40}
                                        ) {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "upsert_Users": [{ "age": 40, "name": "Bob" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "age": 40, "name": "Bob" }, { "age": 40, "name": "Alice" }],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.mutation.upsert.apply")]
fn mutation_upsert_simple_with_no_filter_match_adds_new_doc() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Alice", "age": 40 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        upsert_Users(
                                            filter: {name: {_eq: "Bob"}},
                                            add: {name: "Bob", age: 40},
                                            update: {age: 40}
                                        ) {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "upsert_Users": [{ "age": 40, "name": "Bob" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "age": 40, "name": "Alice" }, { "age": 40, "name": "Bob" }],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.mutation.upsert.errors")]
fn mutation_upsert_simple_with_null_add_input_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        upsert_Users(
                                            filter: {},
                                            add: null,
                                            update: {age: 50}
                                        ) {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(r#"Argument "add" has invalid value <nil>"#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.upsert.errors")]
fn mutation_upsert_simple_with_null_filter_input_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        upsert_Users(
                                            filter: null,
                                            add: {name: "Alice", age: 40},
                                            update: {age: 50}
                                        ) {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(r#"Argument "filter" has invalid value <nil>"#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.upsert.errors")]
fn mutation_upsert_simple_with_null_update_input_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        upsert_Users(
                                            filter: {},
                                            add: {name: "Alice", age: 40},
                                            update: null,
                                        ) {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(r#"Argument "update" has invalid value <nil>"#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.upsert.errors")]
fn mutation_upsert_simple_with_unique_composite_index_and_duplicate_update_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users @index(includes: [{field: "name"}, {field: "age"}], unique: true) {
                        name: String 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Alice", "age": 40 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 50 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        upsert_Users(
                                            filter: {name: {_eq: "Bob"}},
                                            add: {name: "Alice", age: 40},
                                            update: {name: "Alice", age: 40}
                                        ) {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
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
