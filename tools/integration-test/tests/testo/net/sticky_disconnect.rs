//! An explicit `DisconnectPeers` sticks: this node's discovery never dials
//! the peer back until something explicit names it again (#2992).

use integration_test::actions::{
    Actions, ActivePeers, ConnectPeers, DisconnectPeers, NewNode, PeerInfo,
};
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

fn node(name: &str, peer: &str) -> [Actions; 2] {
    [
        NewNode {
            network: true,
            common: on(name),
            ..Default::default()
        }
        .into(),
        PeerInfo {
            common: Common {
                capture: captures([(peer, "/addresses/0")]),
                ..on(name)
            },
            ..Default::default()
        }
        .into(),
    ]
}

fn three_connected_nodes() -> Vec<Actions> {
    let mut actions: Vec<Actions> = [
        node("db0", "peer0"),
        node("db1", "peer1"),
        node("db2", "peer2"),
    ]
    .into_iter()
    .flatten()
    .collect();
    actions.extend([
        ConnectPeers {
            target: "db0".into(),
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        ConnectPeers {
            target: "db2".into(),
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        ActivePeers {
            expected_data: json!({ "addresses": m::unordered(json!([m::r("peer0"), m::r("peer2")])) }),
            common: on("db1"),
            ..Default::default()
        }
        .into(),
    ]);
    actions
}

/// An explicit `ConnectPeers` after the disconnect reaches the peer again.
/// That the connect also lifts the quiet for Kademlia is checked by the p2p
/// unit test `an_explicit_dial_lets_kademlia_dial_a_disconnected_peer_again`:
/// on libp2p db0 rediscovers db1 itself, so no scenario here can see it.
#[test]
#[mantra_macros::req_test("defra.net.peers.disconnect_sticks.explicit_connect")]
fn an_explicit_connect_undoes_a_disconnect() {
    let mut actions = three_connected_nodes();
    actions.extend([
        DisconnectPeers {
            targets: vec!["db0".into()],
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        ConnectPeers {
            target: "db0".into(),
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        ActivePeers {
            expected_data: json!({ "addresses": m::unordered(json!([m::r("peer0"), m::r("peer2")])) }),
            common: on("db1"),
            ..Default::default()
        }
        .into(),
    ]);
    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        without_db: true,
        ..Default::default()
    }
    .execute();
}
