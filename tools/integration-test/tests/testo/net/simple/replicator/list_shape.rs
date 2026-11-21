//! A replicator list reports Go's response shape through both HTTP clients.

use integration_test::actions::{AddCollection, AddReplicator, ListReplicators, NewNode, PeerInfo};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.net.replicator.list_shape")]
fn a_replicator_list_reports_go_shape() {
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
            AddCollection {
                sdl: "type Users { name: String }".into(),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Users { name: String }".into(),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            PeerInfo {
                id_only: true,
                common: Common {
                    capture: captures([("peer0", "/peerID")]),
                    ..on("db1")
                },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                collections: vec!["Users".into()],
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            ListReplicators {
                full: true,
                expected_data: json!([
                    {
                        "ID": m::r("peer0"),
                        "Addresses": [m::unique("replicator_address")],
                        "CollectionIDs": ["Users"],
                        "Status": 0,
                        "LastStatusChange": "0001-01-01T00:00:00Z",
                    },
                ]),
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
