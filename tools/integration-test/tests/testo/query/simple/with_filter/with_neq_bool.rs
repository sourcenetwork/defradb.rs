//! Generated from the conformance scenarios of tests/integration/query/simple/with_filter/with_neq_bool_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.filter.equality.neq")]
fn query_simple_with_bool_not_equals_false_filter_block() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Fred", "Verified": false })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Verified: {_neq: false}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "Name": "John" }, { "Name": "Bob" }] }),
                common: Common {
                    compare: Some(true),
                    unordered: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.query.simple.filter.equality.neq-null")]
fn query_simple_with_bool_not_equals_nil_filter_block() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Fred", "Verified": false })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Verified: {_neq: null}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "Name": "Fred" }, { "Name": "John" }] }),
                common: Common {
                    compare: Some(true),
                    unordered: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.query.simple.filter.equality.neq")]
fn query_simple_with_bool_not_equals_true_filter_block() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Fred", "Verified": false })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Verified: {_neq: true}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "Name": "Fred" }, { "Name": "Bob" }] }),
                common: Common {
                    compare: Some(true),
                    unordered: vec!["/data/Users".into()],
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
