//! Generated from the conformance scenarios of tests/integration/mutation/add/simple_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.add.basic")]
fn mutation_add() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 27 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "_docID": m::r("doc0_0"), "age": 27, "name": "John" }] }),
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
fn mutation_add_given_duplicate_errors() {
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
                doc_map: Some(json!({ "name": "John", "age": 27 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 27 })),
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
#[mantra_macros::req_test("go.mutation.unknown_field")]
fn mutation_add_given_non_existant_field_errors() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "fieldDoesNotExist": 27 })),
                expected_error: Some(
                    "the given field does not exist. Name: fieldDoesNotExist".into(),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
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
#[mantra_macros::req_test("go.mutation.add.basic")]
fn mutation_add_given_empty_input() {
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
                    mutation {
                                        add_Users(input: {}) {
                                            _docID
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "add_Users": [{ "_docID": m::doc_id() }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.add.basic")]
fn mutation_add_with10_collections() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Foo1 {
                        # The name used for the fields is important as the field shortID
                        # is serially assigned based on the alphabetical order of field names.
                        about: String
                        name: String
                    }
                    type Foo2 {
                        name: String
                    }
                    type Foo3 {
                        name: String
                    }
                    type Foo4 {
                        name: String
                    }
                    type Foo5 {
                        name: String
                    }
                    type Foo6 {
                        name: String
                    }
                    type Foo7 {
                        name: String
                    }
                    type Foo8 {
                        name: String
                    }
                    type Foo9 {
                        name: String
                    }
                    type Foo10 {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        add_Foo1(input: {about: "something", name: "John"}) {
                                            about
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "add_Foo1": [{ "about": "something", "name": "John" }] }),
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
