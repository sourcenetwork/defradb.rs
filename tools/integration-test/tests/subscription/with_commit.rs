//! Generated from the conformance scenarios of tests/integration/subscription/with_commit_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, PeerEventsStart, Request, SubscriptionFinish, SubscriptionRequest,
    WaitForUpdate,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.subscription.commits")]
fn commit_subscription_with_add_mutations_return_commits() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String age: Int points: Float verified: Boolean }".into(),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            SubscriptionRequest {
                subscription: "stream".into(),
                query: r#"
                    subscription {
                                        _commits {
                                            cid
                                        }
                                    }
                "#
                .into(),
                expected_count: 2,
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "name": "John", "age": 27, "points": 42.1, "verified": true }),
                ),
                common: Common {
                    retry: true,
                    capture: captures([("DocID0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("DocID0_0"),
                common: Common {
                    capture: captures([("CID0_0_0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "name": "Addo", "age": 31, "points": 42.1, "verified": true }),
                ),
                common: Common {
                    retry: true,
                    capture: captures([("DocID0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("DocID0_1"),
                common: Common {
                    capture: captures([("CID0_1_0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SubscriptionFinish {
                subscription: "stream".into(),
                expected_data: json!({
                    "events": [
                        { "data": { "_commits": [{ "cid": m::r("CID0_0_0") }] } },
                        { "data": { "_commits": [{ "cid": m::r("CID0_1_0") }] } },
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
#[mantra_macros::req_test("go.subscription.commits")]
fn commit_subscription_with_commit_links_add_mutations_valid_links() {
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
                query: r#"
                    subscription {
                                        _commits {
                                            cid
                                            links {
                                                cid
                                                fieldName
                                            }
                                            heads {
                                                cid
                                            }
                                        }
                                    }
                "#.into(),
                expected_count: 2,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        add_User(input: {name: "John", age: 27, points: 42.1, verified: true}) {
                                            name
                                            _version {
                                                links {
                                                    cid
                                                    fieldName
                                                }
                                                heads {
                                                    cid
                                                }
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "add_User": [
                        {
                            "_version": [{ "heads": m::same("0heads"), "links": m::same("0links") }],
                            "name": "John",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        add_User(input: {name: "Addo", age: 31, points: 42.1, verified: true}) {
                                            name
                                            _version {
                                                links {
                                                    cid
                                                    fieldName
                                                }
                                                heads {
                                                    cid
                                                }
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "add_User": [
                        {
                            "_version": [{ "heads": m::same("1heads"), "links": m::same("1links") }],
                            "name": "Addo",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            SubscriptionFinish {
                subscription: "stream".into(),
                expected_data: json!({
                    "events": [
                        {
                            "data": {
                                "_commits": [
                                    {
                                        "cid": m::cid(),
                                        "heads": m::same("0heads"),
                                        "links": m::same("0links"),
                                    },
                                ],
                            },
                        },
                        {
                            "data": {
                                "_commits": [
                                    {
                                        "cid": m::cid(),
                                        "heads": m::same("1heads"),
                                        "links": m::same("1links"),
                                    },
                                ],
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
#[mantra_macros::req_test("go.subscription.commits")]
fn commit_subscription_with_doc_filter_and_multiple_mutations_filtered_doc() {
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
                doc_map: Some(json!({ "name": "John", "age": 21 })),
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
                doc_map: Some(
                    json!({ "name": "Addo", "age": 31, "points": 42.1, "verified": true }),
                ),
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
                query: Text::concat([
                    Text::from(
                        r#"subscription {
					_commits(docID: ""#,
                    ),
                    Text::reference("DocID0_1"),
                    Text::from(
                        r#"") {
						cid
						docID
					}
				}"#,
                    ),
                ]),
                expected_count: 1,
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"mutation {
					update_User(docID: ""#,
                    ),
                    Text::reference("DocID0_0"),
                    Text::from(
                        r#"", input: {verified: true}) {
						_docID
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "update_User": [{ "_docID": m::r("DocID0_0") }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"mutation {
					update_User(docID: ""#,
                    ),
                    Text::reference("DocID0_1"),
                    Text::from(
                        r#"", input: {verified: false}) {
						_docID
						_version {
							cid
						}
					}
				}"#,
                    ),
                ]),
                expected_data: json!({
                    "update_User": [
                        {
                            "_docID": m::same("addoID"),
                            "_version": [
                                { "cid": m::same("updateCID") },
                                { "cid": m::same("createCID") },
                            ],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            SubscriptionFinish {
                subscription: "stream".into(),
                expected_data: json!({
                    "events": [
                        {
                            "data": {
                                "_commits": [
                                    {
                                        "cid": m::same("updateCID"),
                                        "docID": m::same("addoID"),
                                    },
                                ],
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
