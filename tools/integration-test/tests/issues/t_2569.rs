//! Generated from the conformance scenarios of tests/integration/issues/2569_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, NodeOpenHTTP, PeerEventsStart, Request, UpdateDoc, WaitForUpdate,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.issues.pncounter-overflow-serialization")]
fn p2_p_update_with_pn_counter_float_overflow_decrement_prevents_querying() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenHTTP {
                ttl_millis: 60000,
                tick_millis: 1000,
                buckets: 60,
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        points: Float @crdt(type: pncounter)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": -1.7976931348623157e+308 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc"),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "points": -1.7976931348623158e+307 }),
                expected_data: json!({}),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            points
                                        }
                                    }
                "#
                .into(),
                expected_error: Some("unexpected end of JSON input".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.issues.pncounter-overflow-serialization")]
fn p2_p_update_with_pn_counter_float_overflow_prevents_collection_get() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenHTTP {
                ttl_millis: 60000,
                tick_millis: 1000,
                buckets: 60,
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        points: Float @crdt(type: pncounter)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": 1.7976931348623157e+308 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc"),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "points": 1.7976931348623158e+307 }),
                expected_data: json!({}),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc"),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "points": 1 }),
                expected_error: Some("cannot parse JSON: cannot parse empty string".into()),
                common: Common {
                    retry: true,
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
#[mantra_macros::req_test("go.issues.pncounter-overflow-serialization")]
fn p2_p_update_with_pn_counter_float_overflow_increment_prevents_querying() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenHTTP {
                ttl_millis: 60000,
                tick_millis: 1000,
                buckets: 60,
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        points: Float @crdt(type: pncounter)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": 1.7976931348623157e+308 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc"),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "points": 1.7976931348623158e+307 }),
                expected_data: json!({}),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            points
                                        }
                                    }
                "#
                .into(),
                expected_error: Some("unexpected end of JSON input".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
