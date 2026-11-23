//! A subscription whose filter names a field its input type does not define
//! is refused when it opens, with graphql-go's text, and a valid one with a
//! filter variable still streams. Texts measured on Go DefraDB 307364d9d.

use integration_test::actions::{AddCollection, Request, SubscriptionRequest};
use integration_test::test_case::TestCase;
use serde_json::json;

const SDL: &str = "type User { name: String age: Int }";

/// The document IDs of `{name: "John", age: 27}` and `{name: "Addo", age: 31}`.
const JOHN: &str = "bae-a3a40e95-e33c-52f8-98ef-57fcfc43d8d5";
const ADDO: &str = "bae-d7a67c03-1ecc-54c0-87dd-9a23492aeb3c";

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

/// A subscription restricted by a `docID` variable streams only that
/// document's updates.
#[test]
#[mantra_macros::req_test("go.subscription.filtering")]
fn subscription_with_a_doc_id_variable_only_gets_that_documents_updates() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_User(input: [{name: "John", age: 27}, {name: "Addo", age: 31}]) { name } }"#.into(),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                request: Some("subscription($id: ID) { User(docID: $id) { name age } }".into()),
                variables: Some(json!({ "id": ADDO })),
                expected_count: Some(1),
                ..Default::default()
            }
            .into(),
            Request {
                request: format!(r#"mutation {{ update_User(docID: "{JOHN}", input: {{age: 28}}) {{ name }} }}"#).into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: format!(r#"mutation {{ update_User(docID: "{ADDO}", input: {{age: 32}}) {{ name }} }}"#).into(),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                expected_data: json!({
                    "events": [{ "data": { "User": [{ "name": "Addo", "age": 32 }] } }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
