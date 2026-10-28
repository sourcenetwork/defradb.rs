//! Generated from the conformance scenarios of tests/integration/net/simple/peer_replicator/crdt/pncounter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, AddDocumentSubscription, AddReplicator, ConnectPeers, NodeOpenP2P,
    PeerEventsStart, Request, UpdateDoc, WaitForMerge, WaitForUpdate,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.net.crdt.counters_replicator")]
fn p2_p_peer_replicator_with_add_pn_counter_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2P {
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        points: Int @crdt(type: pncounter)
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        points: Int @crdt(type: pncounter)
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        points: Int @crdt(type: pncounter)
                    }
                "#.into(),
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": 0 })),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                common: Common { node: Some("db0".into()), capture: captures([("head0", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": 0 })),
                common: Common { node: Some("db1".into()), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                common: Common { node: Some("db1".into()), capture: captures([("head1", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": 0 })),
                common: Common { node: Some("db2".into()), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                common: Common { node: Some("db2".into()), capture: captures([("head2", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db2".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db1".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad", "points": 3000 })),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_1"),
                common: Common { node: Some("db0".into()), capture: captures([("head3", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForMerge {
                doc_id: Text::reference("doc0_0"),
                cid: Text::reference("head0"),
                expected_data: json!({}),
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForMerge {
                doc_id: Text::reference("doc0_1"),
                cid: Text::reference("head3"),
                expected_data: json!({}),
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            points
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": m::unordered(json!([{ "points": 0 }, { "points": 3000 }])) }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            points
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "points": 0 }] }),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            points
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": m::unordered(json!([{ "points": 0 }, { "points": 3000 }])) }),
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.net.crdt.counters_replicator")]
fn p2_p_peer_replicator_with_update_pn_counter_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2P {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        points: Int @crdt(type: pncounter)
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        points: Int @crdt(type: pncounter)
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        points: Int @crdt(type: pncounter)
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": 10 })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": 10 })),
                common: Common {
                    node: Some("db1".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                common: Common {
                    node: Some("db1".into()),
                    capture: captures([("head1", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": 10 })),
                common: Common {
                    node: Some("db2".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                common: Common {
                    node: Some("db2".into()),
                    capture: captures([("head2", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDocumentSubscription {
                doc_ids: vec![Text::reference("doc0_0")],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db2".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "points": 10 }),
                expected_data: json!({}),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head3", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForMerge {
                doc_id: Text::reference("doc0_0"),
                cid: Text::reference("head3"),
                expected_data: json!({}),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForMerge {
                doc_id: Text::reference("doc0_0"),
                cid: Text::reference("head3"),
                expected_data: json!({}),
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            points
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "points": 20 }] }),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            points
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "points": 20 }] }),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            points
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "points": 20 }] }),
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
