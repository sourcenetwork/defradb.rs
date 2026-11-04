//! Generated from the conformance scenarios of tests/integration/query/simple/with_order_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.order.alias")]
fn query_simple_with_alias_order_on_non_aliased_field_should_order_results() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                                        Users(order: {_alias: {Age: ASC}}) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 19, "Name": "Alice" },
                        { "Age": 21, "Name": "John" },
                        { "Age": 32, "Name": "Bob" },
                        { "Age": 55, "Name": "Carlo" },
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
#[mantra_macros::req_test("go.query.simple.order.alias")]
fn query_simple_with_alias_order_should_order_results() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                                        Users(order: {_alias: {UserAge: ASC}}) {
                                            Name
                                            UserAge: Age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Name": "Alice", "UserAge": 19 },
                        { "Name": "John", "UserAge": 21 },
                        { "Name": "Bob", "UserAge": 32 },
                        { "Name": "Carlo", "UserAge": 55 },
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
#[mantra_macros::req_test("go.query.simple.order.single-field")]
fn query_simple_with_numeric_order_ascending() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                                        Users(order: {Age: ASC}) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 19, "Name": "Alice" },
                        { "Age": 21, "Name": "John" },
                        { "Age": 32, "Name": "Bob" },
                        { "Age": 55, "Name": "Carlo" },
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
#[mantra_macros::req_test("go.query.simple.order.single-field")]
fn query_simple_with_numeric_order_descending() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                                        Users(order: {Age: DESC}) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 55, "Name": "Carlo" },
                        { "Age": 32, "Name": "Bob" },
                        { "Age": 21, "Name": "John" },
                        { "Age": 19, "Name": "Alice" },
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
#[mantra_macros::req_test("go.query.simple.order.multi-field")]
fn query_simple_with_multiple_order_fields_asc_and_asc_should_order_correctly() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                doc_map: Some(json!({ "Name": "Alice", "Age": 38 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 22 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 30 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 24 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: [{Name: ASC}, {Age: ASC}]) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 22, "Name": "Alice" },
                        { "Age": 24, "Name": "Alice" },
                        { "Age": 38, "Name": "Alice" },
                        { "Age": 30, "Name": "Bob" },
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
#[mantra_macros::req_test("go.query.simple.order.multi-field")]
fn query_simple_with_multiple_order_fields_acs_and_desc_should_order_correctly() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                doc_map: Some(json!({ "Name": "Alice", "Age": 38 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 22 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 30 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 24 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: [{Name: ASC}, {Age: DESC}]) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 38, "Name": "Alice" },
                        { "Age": 24, "Name": "Alice" },
                        { "Age": 22, "Name": "Alice" },
                        { "Age": 30, "Name": "Bob" },
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
#[mantra_macros::req_test("go.query.simple.order.multi-field")]
fn query_simple_with_multiple_order_fields_desc_and_asc_should_order_correctly() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                doc_map: Some(json!({ "Name": "Alice", "Age": 38 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 22 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 30 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 24 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: [{Name: DESC}, {Age: ASC}]) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 30, "Name": "Bob" },
                        { "Age": 22, "Name": "Alice" },
                        { "Age": 24, "Name": "Alice" },
                        { "Age": 38, "Name": "Alice" },
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
#[mantra_macros::req_test("go.query.simple.order.multi-field")]
fn query_simple_with_multiple_order_fields_decs_and_desc_should_order_correctly() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                doc_map: Some(json!({ "Name": "Alice", "Age": 38 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 22 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 30 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 24 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: [{Name: DESC}, {Age: DESC}]) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 30, "Name": "Bob" },
                        { "Age": 38, "Name": "Alice" },
                        { "Age": 24, "Name": "Alice" },
                        { "Age": 22, "Name": "Alice" },
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
#[mantra_macros::req_test("go.query.simple.order.single-field")]
fn query_simple_wit_float64_order_descending() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                                        Name: String
                                        HeightM: Float
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "HeightM": 21 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "HeightM": 32 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Carlo", "HeightM": 55 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "HeightM": 19 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: {HeightM: DESC}) {
                                            Name
                                            HeightM
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "HeightM": 55.0, "Name": "Carlo" },
                        { "HeightM": 32.0, "Name": "Bob" },
                        { "HeightM": 21.0, "Name": "John" },
                        { "HeightM": 19.0, "Name": "Alice" },
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
#[mantra_macros::req_test("go.query.simple.order.alias")]
fn query_simple_with_alias_order_on_non_existant_field_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                                        Users(order: {_alias: {UserAge: ASC}}) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_error: Some("field or alias not found. Name: UserAge".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.order.single-field")]
fn query_simple_with_blob_order_ascending() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                                        Name: String
                                        Raw: Blob
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Raw": "21" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Raw": "32" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Carlo", "Raw": "55" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Raw": "19" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: {Raw: ASC}) {
                                            Name
                                            Raw
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Name": "Alice", "Raw": "19" },
                        { "Name": "John", "Raw": "21" },
                        { "Name": "Bob", "Raw": "32" },
                        { "Name": "Carlo", "Raw": "55" },
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
#[mantra_macros::req_test("go.query.simple.order.single-field")]
fn query_simple_with_blob_order_descending() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                                        Name: String
                                        Raw: Blob
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Raw": "21" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Raw": "32" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Carlo", "Raw": "55" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Raw": "19" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: {Raw: DESC}) {
                                            Name
                                            Raw
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Name": "Carlo", "Raw": "55" },
                        { "Name": "Bob", "Raw": "32" },
                        { "Name": "John", "Raw": "21" },
                        { "Name": "Alice", "Raw": "19" },
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
#[mantra_macros::req_test("go.query.simple.order.alias")]
fn query_simple_with_compound_alias_order_should_order_results() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                doc_map: Some(json!({ "Name": "John", "Age": 21, "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 21, "Verified": false })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Carlo", "Age": 55, "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 19, "Verified": false })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: [{_alias: {userAge: DESC}}, {_alias: {isVerified: ASC}}]) {
                                            Name
                                            userAge: Age
                                            isVerified: Verified
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Users": [
                        { "Name": "Carlo", "isVerified": true, "userAge": 55 },
                        { "Name": "Bob", "isVerified": false, "userAge": 21 },
                        { "Name": "John", "isVerified": true, "userAge": 21 },
                        { "Name": "Alice", "isVerified": false, "userAge": 19 },
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
#[mantra_macros::req_test("go.query.simple.order.single-field")]
fn query_simple_with_date_time_order_ascending() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                doc_map: Some(
                    json!({ "Name": "John", "Age": 21, "CreatedAt": "2021-07-23T03:46:56-05:00" }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "Bob", "Age": 32, "CreatedAt": "2032-07-23T03:46:56-05:00" }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "Carlo", "Age": 55, "CreatedAt": "2055-07-23T03:46:56-05:00" }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "Alice", "Age": 19, "CreatedAt": "2019-07-23T03:46:56-05:00" }),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: {CreatedAt: ASC}) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 19, "Name": "Alice" },
                        { "Age": 21, "Name": "John" },
                        { "Age": 32, "Name": "Bob" },
                        { "Age": 55, "Name": "Carlo" },
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
#[mantra_macros::req_test("go.query.simple.order.single-field")]
fn query_simple_with_date_time_order_descending() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                doc_map: Some(
                    json!({ "Name": "John", "Age": 21, "CreatedAt": "2021-07-23T03:46:56-05:00" }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "Bob", "Age": 32, "CreatedAt": "2032-07-23T03:46:56-05:00" }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "Carlo", "Age": 55, "CreatedAt": "2055-07-23T03:46:56-05:00" }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "Alice", "Age": 19, "CreatedAt": "2019-07-23T03:46:56-05:00" }),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: {CreatedAt: DESC}) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 55, "Name": "Carlo" },
                        { "Age": 32, "Name": "Bob" },
                        { "Age": 21, "Name": "John" },
                        { "Age": 19, "Name": "Alice" },
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
#[mantra_macros::req_test("go.query.simple.order.alias")]
fn query_simple_with_empty_alias_order_should_do_nothing() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                                        Users(order: {_alias: {}}) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 32, "Name": "Bob" },
                        { "Age": 21, "Name": "John" },
                        { "Age": 55, "Name": "Carlo" },
                        { "Age": 19, "Name": "Alice" },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.order.single-field")]
fn query_simple_with_empty_order() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
            Request {
                request: r#"
                    query {
                                        Users(order: {}) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 32, "Name": "Bob" },
                        { "Age": 21, "Name": "John" },
                        { "Age": 55, "Name": "Carlo" },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.order.single-field")]
fn query_simple_with_float32_order_ascending() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                                        Name: String
                                        Points: Float32
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Points": 21 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Points": 32 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Carlo", "Points": 55 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Points": 19 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: {Points: ASC}) {
                                            Name
                                            Points
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Name": "Alice", "Points": 19.0 },
                        { "Name": "John", "Points": 21.0 },
                        { "Name": "Bob", "Points": 32.0 },
                        { "Name": "Carlo", "Points": 55.0 },
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
#[mantra_macros::req_test("go.query.simple.order.single-field")]
fn query_simple_with_float32_order_descending() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                                        Name: String
                                        Points: Float32
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Points": 21 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Points": 32 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Carlo", "Points": 55 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Points": 19 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: {Points: DESC}) {
                                            Name
                                            Points
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Name": "Carlo", "Points": 55.0 },
                        { "Name": "Bob", "Points": 32.0 },
                        { "Name": "John", "Points": 21.0 },
                        { "Name": "Alice", "Points": 19.0 },
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
#[mantra_macros::req_test("go.query.simple.order.single-field")]
fn query_simple_with_float64_order_ascending() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                                        Name: String
                                        HeightM: Float
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "HeightM": 21 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "HeightM": 32 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Carlo", "HeightM": 55 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "HeightM": 19 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: {HeightM: ASC}) {
                                            Name
                                            HeightM
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "HeightM": 19.0, "Name": "Alice" },
                        { "HeightM": 21.0, "Name": "John" },
                        { "HeightM": 32.0, "Name": "Bob" },
                        { "HeightM": 55.0, "Name": "Carlo" },
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
#[mantra_macros::req_test("go.query.simple.order.alias")]
fn query_simple_with_int_alias_order_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                                        Users(order: {_alias: 1}) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_error: Some("invalid order input".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.order.alias")]
fn query_simple_with_invalid_alias_order_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                                        Users(order: {_alias: {UserAge: invalid}}) {
                                            Name
                                            UserAge: Age
                                        }
                                    }
                "#
                .into(),
                expected_error: Some("invalid order direction".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.order.validation")]
fn query_simple_with_invalid_order_enum_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
            Request {
                request: r#"
                    query {
                                        Users(order: {Age: INVALID}) {
                                            Name
                                            Age
                                            Verified
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(r#"Argument "order" has invalid value {Age: INVALID}"#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.order.validation")]
fn query_simple_with_multiple_order_fields_nested_within_multple_fields_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
            Request {
                request: r#"
                    query {
                                        Users(order: [{Age: ASC}, {Age: ASC, Name: DESC}]) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_error: Some("each order argument can only define one field".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.order.validation")]
fn query_simple_with_multiple_order_fields_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
            Request {
                request: r#"
                    query {
                                        Users(order: {Age: ASC, Name: DESC}) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_error: Some("each order argument can only define one field".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.order.alias")]
fn query_simple_with_null_alias_order_should_do_nothing() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                                        Users(order: {_alias: null}) {
                                            Name
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 32, "Name": "Bob" },
                        { "Age": 21, "Name": "John" },
                        { "Age": 55, "Name": "Carlo" },
                        { "Age": 19, "Name": "Alice" },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.order.multi-field")]
fn query_simple_with_numeric_order_descending_and_boolean_order_ascending() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
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
                doc_map: Some(json!({ "Name": "John", "Age": 21, "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 21, "Verified": false })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Carlo", "Age": 55, "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 19, "Verified": false })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: [{Age: DESC}, {Verified: ASC}]) {
                                            Name
                                            Age
                                            Verified
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 55, "Name": "Carlo", "Verified": true },
                        { "Age": 21, "Name": "Bob", "Verified": false },
                        { "Age": 21, "Name": "John", "Verified": true },
                        { "Age": 19, "Name": "Alice", "Verified": false },
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
