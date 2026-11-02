//! Generated from the conformance scenarios of tests/integration/query/simple/with_multiple_types_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.select.collection-isolation")]
fn simple_with_eight_dummy_types_after() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }

                    type Type0 {
                        f: String
                    }
                    type Type1 {
                        f: String
                    }
                    type Type2 {
                        f: String
                    }
                    type Type3 {
                        f: String
                    }
                    type Type4 {
                        f: String
                    }
                    type Type5 {
                        f: String
                    }
                    type Type6 {
                        f: String
                    }
                    type Type7 {
                        f: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.simple.select.collection-isolation")]
fn simple_with_eight_dummy_types_before_in_split_declaration() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Type0 {
                        f: String
                    }
                    type Type1 {
                        f: String
                    }
                    type Type2 {
                        f: String
                    }
                    type Type3 {
                        f: String
                    }
                    type Type4 {
                        f: String
                    }
                    type Type5 {
                        f: String
                    }
                    type Type6 {
                        f: String
                    }
                    type Type7 {
                        f: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.simple.select.collection-isolation")]
fn simple_with_eight_dummy_types_before() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Type0 {
                        f: String
                    }
                    type Type1 {
                        f: String
                    }
                    type Type2 {
                        f: String
                    }
                    type Type3 {
                        f: String
                    }
                    type Type4 {
                        f: String
                    }
                    type Type5 {
                        f: String
                    }
                    type Type6 {
                        f: String
                    }
                    type Type7 {
                        f: String
                    }

                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.simple.select.collection-isolation")]
fn simple_with_seven_dummy_types_before_and_one_after() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Type0 {
                        f: String
                    }
                    type Type1 {
                        f: String
                    }
                    type Type2 {
                        f: String
                    }
                    type Type3 {
                        f: String
                    }
                    type Type4 {
                        f: String
                    }
                    type Type5 {
                        f: String
                    }
                    type Type6 {
                        f: String
                    }

                    type User {
                        name: String
                    }

                    type Type7 {
                        f: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.simple.select.collection-isolation")]
fn simple_with_seven_dummy_types_before() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Type0 {
                        f: String
                    }
                    type Type1 {
                        f: String
                    }
                    type Type2 {
                        f: String
                    }
                    type Type3 {
                        f: String
                    }
                    type Type4 {
                        f: String
                    }
                    type Type5 {
                        f: String
                    }
                    type Type6 {
                        f: String
                    }

                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
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
