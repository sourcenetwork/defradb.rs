//! An explicit `DisconnectPeers` sticks: this node's discovery never dials
//! the peer back until something explicit names it again (#2992). On pinc
//! the peer it hung up on does not dial back either.

use integration_test::actions::{
    Actions, ActivePeers, ConnectPeers, DisconnectPeers, NewNode, PeerInfo, Wait,
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

/// Longer than every discovery dial that follows a connect: a peer exchange
/// list lands within milliseconds of the connect that sends it.
const DISCOVERY_WINDOW_MILLIS: u64 = 1500;

/// Printed where a pinc-only scenario does not run: libp2p's close carries no
/// reason, so the peer hung up on still dials back there.
const LIBP2P_SKIP: &str =
    "skipped: pinc only; set DEFRA_TRANSPORT=pinc and build with --features pinc";

/// db1 disconnects both its peers, and neither comes back through discovery,
/// whichever side's discovery would dial. pinc only: libp2p's close carries
/// no reason, so there the peers' own Kademlia dials db1 back.
#[test]
#[mantra_macros::req_test("defra.net.peers.disconnect_sticks.discovery")]
fn a_disconnected_peer_is_not_dialed_back_by_discovery() {
    if !integration_test::transport_choice::TransportChoice::from_env().is_pinc() {
        eprintln!("{LIBP2P_SKIP}");
        return;
    }
    let mut actions = three_connected_nodes();
    actions.extend([
        DisconnectPeers {
            targets: vec!["db0".into(), "db2".into()],
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        Wait {
            millis: Some(DISCOVERY_WINDOW_MILLIS),
            ..Default::default()
        }
        .into(),
        ActivePeers {
            expected_data: json!({ "addresses": m::unordered(json!([])) }),
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

/// db1 disconnects db0, then reconnects to db2, which hands it a fresh list
/// naming db0: db1's own discovery leaves db0 alone. pinc only, as above.
#[test]
#[mantra_macros::req_test("defra.net.peers.disconnect_sticks.discovery")]
fn a_fresh_list_does_not_redial_a_disconnected_peer() {
    if !integration_test::transport_choice::TransportChoice::from_env().is_pinc() {
        eprintln!("{LIBP2P_SKIP}");
        return;
    }
    let mut actions = three_connected_nodes();
    actions.extend([
        // db2 must hold db0 for its fresh list to name it: db1's peer
        // exchange introduced them, so wait for that dial to land.
        Wait {
            millis: Some(DISCOVERY_WINDOW_MILLIS),
            ..Default::default()
        }
        .into(),
        ActivePeers {
            expected_data: json!({ "addresses": m::unordered(json!([m::r("peer0"), m::r("peer1")])) }),
            common: on("db2"),
            ..Default::default()
        }
        .into(),
        DisconnectPeers {
            targets: vec!["db0".into(), "db2".into()],
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
        Wait {
            millis: Some(DISCOVERY_WINDOW_MILLIS),
            ..Default::default()
        }
        .into(),
        ActivePeers {
            expected_data: json!({ "addresses": m::unordered(json!([m::r("peer2")])) }),
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
