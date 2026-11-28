//! A restarted target of a symmetric replicator mesh keeps receiving its peer's updates.

use integration_test::actions::{
    AddCollection, AddDoc, AddReplicator, ConnectPeers, NewNode, Request, Restart, UpdateDoc,
    WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.net.replicator.symmetric_restart")]
fn a_restarted_replicator_target_keeps_accepting_its_peers_updates() {
    let sdl = "type Item { counter: Int }";
    let read = "query { Item { counter } }";
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                database_dir: Some("".into()),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                database_dir: Some("".into()),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: sdl.into(),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: sdl.into(),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db0".into(),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Item".into(),
                doc_map: Some(json!({ "counter": 1 })),
                common: Common {
                    retry: true,
                    capture: captures([("item", "/docID")]),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: read.into(),
                expected_data: json!({ "Item": [{ "counter": 1 }] }),
                common: Common {
                    eventually: Some(Eventually::default()),
                    ..on("db1")
                },
                ..Default::default()
            }
            .into(),
            Restart {
                stop: true,
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            Restart {
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db1".into(),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Item".into(),
                doc_id: Text::reference("item"),
                doc_map: json!({ "counter": 2 }),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            Request {
                request: read.into(),
                expected_data: json!({ "Item": [{ "counter": 2 }] }),
                common: Common {
                    eventually: Some(Eventually::default()),
                    ..on("db1")
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
