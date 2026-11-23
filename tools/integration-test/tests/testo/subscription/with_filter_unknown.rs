//! A subscription whose filter names a field its input type does not define
//! is refused when it opens, with graphql-go's text, and a valid one with a
//! filter variable still streams. Texts measured on Go DefraDB 307364d9d.

use integration_test::actions::{AddCollection, Request, SubscriptionRequest};
use integration_test::test_case::TestCase;
use serde_json::json;

const SDL: &str = "type User { name: String age: Int }";

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn subscription_with_an_unknown_filter_field_is_refused_on_open() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                request: Some("subscription { User(filter: {nope: {_eq: 1}}) { name } }".into()),
                expected_error: Some(
                    "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field."
                        .into(),
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
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn subscription_with_an_unknown_filter_variable_field_is_refused_on_open() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                request: Some("subscription($f: UserFilterArg) { User(filter: $f) { name } }".into()),
                variables: Some(json!({ "f": { "nope": { "_eq": 1 } } })),
                expected_error: Some(
                    "Variable \"$f\" got invalid value {\"nope\":{\"_eq\":1}}.\nIn field \"nope\": Unknown field."
                        .into(),
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
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn subscription_with_a_valid_filter_variable_streams() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                request: Some(
                    "subscription($f: UserFilterArg) { User(filter: $f) { name } }".into(),
                ),
                variables: Some(json!({ "f": { "age": { "_gt": 20 } } })),
                expected_count: Some(1),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_User(input: {name: "John", age: 27}) { name } }"#.into(),
                expected_data: json!({ "add_User": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                expected_data: json!({
                    "events": [{ "data": { "User": [{ "name": "John" }] } }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
