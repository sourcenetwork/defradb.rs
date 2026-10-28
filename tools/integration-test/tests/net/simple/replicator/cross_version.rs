//! Generated from the conformance scenarios of tests/integration/net/simple/replicator/cross_version_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, AddReplicator, NodeOpenP2PIdentity, NodeOpenP2PRelease, PeerEventsStart,
    Request, WaitForUpdate,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.net.replicator.cross_version")]
fn p2_p_cross_version_head_to_v1_doc_syncs() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2PIdentity {
                node_identity_seed: Some(0),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NodeOpenP2PRelease {
                release_version: "v1.0.0".into(),
                node_identity_seed: 1,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc"),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { User { name } }".into(),
                expected_data: json!(m::object(json!({
                    "User": m::first_where("name", "Alice", json!(m::object(json!({ "name": "Alice" })))),
                }))),
                common: Common { node: Some("db1".into()), eventually: Some(Eventually { timeout_millis: Some(30000), interval_millis: Some(200), ..Default::default() }), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { User { name } }".into(),
                expected_data: json!({ "User": [{ "name": "Alice" }] }),
                common: Common { node: Some("db1".into()), ..Default::default() },
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
#[mantra_macros::req_test("go.net.replicator.cross_version")]
fn p2_p_cross_version_v1_to_head_doc_syncs() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2PIdentity {
                node_identity_seed: Some(0),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NodeOpenP2PRelease {
                release_version: "v1.0.0".into(),
                node_identity_seed: 1,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db0".into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                common: Common { node: Some("db1".into()), retry: true, capture: captures([("doc", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { User { name } }".into(),
                expected_data: json!(m::object(json!({
                    "User": m::first_where("name", "Bob", json!(m::object(json!({ "name": "Bob" })))),
                }))),
                common: Common { node: Some("db0".into()), eventually: Some(Eventually { timeout_millis: Some(30000), interval_millis: Some(200), ..Default::default() }), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { User { name } }".into(),
                expected_data: json!({ "User": [{ "name": "Bob" }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
