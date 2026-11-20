//! A replicator added while the source has no collections (#2702). Go stores
//! it with an empty collection list, lists it, keeps it across a restart, and
//! pushes it nothing (internal/db/p2p/replicator.go, p2p.go pushLogToReplicators).

use integration_test::actions::{
    AddCollection, AddDoc, AddReplicator, Close, ListReplicators, NewNode, PeerInfo, Request,
    Start, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("go.net.replicator.push")]
fn a_replicator_with_no_collections_is_listed_after_a_restart() {
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
            PeerInfo {
                id_only: true,
                common: Common {
                    capture: captures([("peer1", "/peerID")]),
                    ..on("db1")
                },
                ..Default::default()
            }
            .into(),
            ListReplicators {
                expected_data: json!({ "peerIDs": m::unordered(json!([m::r("peer1")])) }),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            Close {
                keep_data: true,
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: false,
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            ListReplicators {
                expected_data: json!({ "peerIDs": m::unordered(json!([m::r("peer1")])) }),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

/// Best effort: pushes to different peers are not strictly ordered, so db1
/// could in principle still be receiving after db2 has synced. The push-layer
/// unit test `a_replicator_with_no_collections_gets_no_push_job` is the proof.
#[test]
#[mantra_macros::req_test("go.net.replicator.push")]
fn a_replicator_with_no_collections_receives_no_documents() {
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
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: on("db2"),
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
            WaitForPeersEvents {
                start: true,
                common: on("db2"),
                ..Default::default()
            }
            .into(),
            // No collection exists yet, so db1 replicates none.
            AddReplicator {
                target: "db1".into(),
                common: on("db0"),
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
            AddCollection {
                sdl: sdl.into(),
                common: on("db2"),
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db2".into(),
                collections: vec!["Users".into()],
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John" })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Fred" })),
                common: Common {
                    retry: true,
                    capture: captures([("doc1", "/docID")]),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common {
                    capture: captures([("head0", "/cid")]),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc1")),
                common: Common {
                    capture: captures([("head1", "/cid")]),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                cid: Some(Text::reference("head0")),
                expected_data: json!({}),
                common: on("db2"),
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc1")),
                cid: Some(Text::reference("head1")),
                expected_data: json!({}),
                common: on("db2"),
                ..Default::default()
            }
            .into(),
            Request {
                request: users.into(),
                expected_data: json!({ "Users": m::unordered(json!([{ "Name": "John" }, { "Name": "Fred" }])) }),
                common: on("db2"),
                ..Default::default()
            }
            .into(),
            Request {
                request: users.into(),
                expected_data: json!({ "Users": [] }),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
