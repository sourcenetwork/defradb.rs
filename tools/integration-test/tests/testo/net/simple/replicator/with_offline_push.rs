//! A replicator parks a push while its target is offline and delivers it when
//! the target returns. The counters are defradb.rs's `p2p status`, so the
//! unsupported counter actions are reported by clients that lack them.

use integration_test::actions::{
    AddCollection, AddDoc, AddReplicator, Close, ConnectPeers, NewNode, Request, Start,
    SyncCounters, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
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
#[mantra_macros::req_test("defra.net.push.parked_offline")]
fn a_push_to_a_stopped_peer_is_parked_until_it_restarts() {
    let sdl = r#"
        type Users {
            Name: String
        }
    "#;
    let users = r#"
        query {
            Users {
                Name
            }
        }
    "#;
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
            Close {
                keep_data: true,
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John" })),
                common: Common {
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            SyncCounters {
                at_most: [("push_backlog/failed_total".to_owned(), 0)]
                    .into_iter()
                    .collect(),
                at_least: [("push_backlog/parked_total".to_owned(), 1)]
                    .into_iter()
                    .collect(),
                expected_data: m::object(json!({
                    "push_backlog": m::object(json!({
                        "failed_total": 0,
                        "parked_total": m::positive(),
                    })),
                })),
                common: Common {
                    eventually: Some(Eventually {
                        timeout_millis: Some(10_000),
                        interval_millis: Some(200),
                    }),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: false,
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
            Request {
                request: users.into(),
                expected_data: json!({ "Users": [{ "Name": "John" }] }),
                common: Common {
                    eventually: Some(Eventually {
                        timeout_millis: Some(10_000),
                        interval_millis: Some(200),
                    }),
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
