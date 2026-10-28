//! Generated from the conformance scenarios of tests/integration/query/simple/with_filter/with_and_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.filter.logical.and-or")]
fn query_simple_with_int_greater_than_and_int_less_than_filter() {
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
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 21 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 32 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Carlo", "Age": 55 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 19 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {_and: [{Age: {_gt: 20}}, {Age: {_lt: 50}}]}) {
                                            Name
                                            Age
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "Age": 32, "Name": "Bob" }, { "Age": 21, "Name": "John" }] }),
                common: Common { compare: Some(true), unordered: vec!["/data/Users".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.filter.logical.and-or")]
fn query_simple_with_inline_int_array_greater_than_and_less_than_filter_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                                        Name: String
                                        FavoriteNumbers: [Int!]
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "FavoriteNumbers": [0, 10, 20] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "FavoriteNumbers": [30, 40, 50] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {_and: [
                                            {FavoriteNumbers: {_all: {_geq: 0}}},
                                            {FavoriteNumbers: {_all: {_lt: 30}}},
                                        ]}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "Name": "Bob" }] }),
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
