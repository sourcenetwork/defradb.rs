//! Generated from the conformance scenarios of tests/integration/subscription/subscription_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, DeleteDoc, Request, SubscriptionRequest, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.subscription.document-events")]
fn subscription_with_add_mutations() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String age: Int points: Float verified: Boolean }".into(),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                request: Some(r#"
                    subscription {
                                        User {
                                            _docID
                                            name
                                            age
                                        }
                                    }
                "#.into()),
                expected_count: Some(2),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        add_User(input: {name: "John", age: 27, points: 42.1, verified: true}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "add_User": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        add_User(input: {name: "Addo", age: 31, points: 42.1, verified: true}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "add_User": [{ "name": "Addo" }] }),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                expected_data: json!({
                    "events": [
                        {
                            "data": {
                                "User": [{ "_docID": m::doc_id(), "age": 27, "name": "John" }],
                            },
                        },
                        {
                            "data": {
                                "User": [{ "_docID": m::doc_id(), "age": 31, "name": "Addo" }],
                            },
                        },
                    ],
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
#[mantra_macros::req_test("go.subscription.close")]
fn subscription_with_close_wont_block() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String age: Int points: Float verified: Boolean }".into(),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                request: Some(
                    r#"
                    subscription {
                                        User{
                                            name
                                            age
                                        }
                                    }
                "#
                    .into(),
                ),
                expected_count: Some(-1),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.subscription.document-events")]
fn subscription_with_counter_crdt_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        counter: Int @crdt(type: pcounter)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                request: Some(
                    r#"
                    subscription {
                                        User {
                                            counter
                                        }
                                    }
                "#
                    .into(),
                ),
                expected_count: Some(2),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "counter": 1 })),
                common: Common {
                    retry: true,
                    capture: captures([("DocID0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("DocID0_0"),
                doc_map: json!({ "counter": 1 }),
                expected_data: json!({}),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                expected_data: json!({
                    "events": [
                        { "data": { "User": [{ "counter": 1 }] } },
                        { "data": { "User": [{ "counter": 2 }] } },
                    ],
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
#[mantra_macros::req_test("go.subscription.document-events")]
fn subscription_with_delete_operation_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
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
            SubscriptionRequest {
                subscription: "stream".into(),
                request: Some(
                    r#"
                    subscription {
                                        User (showDeleted: true) { 
                                            name
                                            _deleted
                                        }
                                    }
                "#
                    .into(),
                ),
                expected_count: Some(3),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    retry: true,
                    capture: captures([("DocID0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("DocID0_0"),
                doc_map: json!({ "name": "Johny" }),
                expected_data: json!({}),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "User".into(),
                doc_id: Text::reference("DocID0_0"),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                expected_data: json!({
                    "events": [
                        { "data": { "User": [{ "_deleted": false, "name": "John" }] } },
                        { "data": { "User": [{ "_deleted": false, "name": "Johny" }] } },
                        { "data": { "User": [{ "_deleted": true, "name": "Johny" }] } },
                    ],
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
#[mantra_macros::req_test("go.subscription.filtering")]
fn subscription_with_doc_id_filter_should_only_get_updates_for_that_doc_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String age: Int points: Float verified: Boolean }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 27, "name": "John" })),
                common: Common { retry: true, capture: captures([("DocID0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 31, "name": "Addo" })),
                common: Common { retry: true, capture: captures([("DocID0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                request: Some(Text::concat([Text::from(r#"subscription {
					User(docID: ""#), Text::reference("DocID0_1"), Text::from(r#"") {
						name
						age
					}
				}"#)])),
                expected_count: Some(1),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("DocID0_0"),
                doc_map: json!({ "age": 28 }),
                expected_data: json!({}),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("DocID0_1"),
                doc_map: json!({ "age": 32 }),
                expected_data: json!({}),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                expected_data: json!({ "events": [{ "data": { "User": [{ "age": 32, "name": "Addo" }] } }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.subscription.filtering")]
fn subscription_with_filter_and_add_mutations() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String age: Int points: Float verified: Boolean }".into(),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                request: Some(r#"
                    subscription {
                                        User(filter: {age: {_lt: 30}}) {
                                            name
                                            age
                                        }
                                    }
                "#.into()),
                expected_count: Some(1),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        add_User(input: {name: "John", age: 27, points: 42.1, verified: true}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "add_User": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        add_User(input: {name: "Addo", age: 31, points: 42.1, verified: true}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "add_User": [{ "name": "Addo" }] }),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                expected_data: json!({ "events": [{ "data": { "User": [{ "age": 27, "name": "John" }] } }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.subscription.filtering")]
fn subscription_with_filter_and_one_add_mutation_outside_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String age: Int points: Float verified: Boolean }".into(),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                request: Some(r#"
                    subscription {
                                        User(filter: {age: {_gt: 30}}) {
                                            _docID
                                            name
                                            age
                                        }
                                    }
                "#.into()),
                expected_count: Some(0),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        add_User(input: {name: "John", age: 27, points: 42.1, verified: true}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "add_User": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                expected_data: json!({ "events": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.subscription.filtering")]
fn subscription_with_filter_and_one_add_mutation() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String age: Int points: Float verified: Boolean }".into(),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                request: Some(r#"
                    subscription {
                                        User(filter: {age: {_lt: 30}}) {
                                            name
                                            age
                                        }
                                    }
                "#.into()),
                expected_count: Some(1),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        add_User(input: {name: "John", age: 27, points: 42.1, verified: true}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "add_User": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                expected_data: json!({ "events": [{ "data": { "User": [{ "age": 27, "name": "John" }] } }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.subscription.document-events")]
fn subscription_with_update_all_mutations() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String age: Int points: Float verified: Boolean }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "name": "John", "age": 27, "verified": true, "points": 42.1 }),
                ),
                common: Common {
                    retry: true,
                    capture: captures([("DocID0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 31, "verified": true, "points": 50 })),
                common: Common {
                    retry: true,
                    capture: captures([("DocID0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                request: Some(
                    r#"
                    subscription {
                                        User {
                                            name
                                            age
                                            points
                                        }
                                    }
                "#
                    .into(),
                ),
                expected_count: Some(2),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        update_User(input: {points: 55}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "update_User": [{ "name": "John" }, { "name": "Addo" }] }),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                expected_data: json!({
                    "events": [
                        {
                            "data": { "User": [{ "age": 27, "name": "John", "points": 55.0 }] },
                        },
                        {
                            "data": { "User": [{ "age": 31, "name": "Addo", "points": 55.0 }] },
                        },
                    ],
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
#[mantra_macros::req_test("go.subscription.document-events")]
fn subscription_with_update_mutations() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String age: Int points: Float verified: Boolean }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 27, "verified": true, "points": 42.1 })),
                common: Common { retry: true, capture: captures([("DocID0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo", "age": 35, "verified": true, "points": 50 })),
                common: Common { retry: true, capture: captures([("DocID0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                request: Some(r#"
                    subscription {
                                        User {
                                            name
                                            age
                                            points
                                        }
                                    }
                "#.into()),
                expected_count: Some(1),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        update_User(filter: {name: {_eq: "John"}}, input: {points: 45}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "update_User": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                expected_data: json!({
                    "events": [
                        {
                            "data": { "User": [{ "age": 27, "name": "John", "points": 45.0 }] },
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
