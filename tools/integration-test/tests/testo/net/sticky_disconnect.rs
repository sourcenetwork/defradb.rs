//! An explicit `DisconnectPeers` sticks: this node's discovery never dials
//! the peer back until something explicit names it again (#2992). On pinc
//! the peer it hung up on does not dial back either.

use integration_test::actions::{
    Actions, ActivePeers, ConnectPeers, DisconnectPeers, NewNode, PeerInfo, WaitForPeersEvents,
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

fn watch(name: &str) -> Actions {
    WaitForPeersEvents {
        start: true,
        common: on(name),
        ..Default::default()
    }
    .into()
}

/// Wait until `name` read a peer exchange list from each of `from` and every
/// discovery dial the list scheduled landed or failed.
fn learned(name: &str, from: &[&str]) -> Actions {
    WaitForPeersEvents {
        peers_learned: Some(from.iter().map(|peer| (*peer).into()).collect()),
        common: on(name),
        ..Default::default()
    }
    .into()
}

/// db1 connects db0 and db2. With `watched`, each node records its events
/// from the start, and the scenario waits for db2 to have read db1's list
/// naming db0 and dialed db0: db0 and db2 are then connected too.
fn three_connected_nodes(watched: bool) -> Vec<Actions> {
    let mut actions = Vec::new();
    for (name, peer) in [("db0", "peer0"), ("db1", "peer1"), ("db2", "peer2")] {
        actions.extend(node(name, peer));
        if watched {
            actions.push(watch(name));
        }
    }
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
    if watched {
        actions.push(learned("db2", &["db1"]));
    }
    actions
}

/// An explicit `ConnectPeers` after the disconnect reaches the peer again.
/// That the connect also lifts the quiet for Kademlia is checked by the p2p
/// unit test `an_explicit_dial_lets_kademlia_dial_a_disconnected_peer_again`:
/// on libp2p db0 rediscovers db1 itself, so no scenario here can see it.
#[test]
#[mantra_macros::req_test("defra.net.peers.disconnect_sticks.explicit_connect")]
fn an_explicit_connect_undoes_a_disconnect() {
    let mut actions = three_connected_nodes(false);
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

/// Printed where a pinc-only scenario does not run: libp2p's close carries no
/// reason, so the peer hung up on still dials back there.
const LIBP2P_SKIP: &str =
    "skipped: pinc only; set DEFRA_TRANSPORT=pinc and build with --features pinc";

/// db1 disconnects both its peers. A new node, db3, then connects db1 and
/// db0, and hands db0 and, through db0's list, db2 a list naming db1: neither
/// dials db1 back. pinc only: libp2p's close carries no reason, so there the
/// peers' own Kademlia dials db1 back.
#[test]
#[mantra_macros::req_test("defra.net.peers.disconnect_sticks.discovery")]
fn a_disconnected_peer_is_not_dialed_back_by_discovery() {
    if !integration_test::transport_choice::TransportChoice::from_env().is_pinc() {
        eprintln!("{LIBP2P_SKIP}");
        return;
    }
    let mut actions = three_connected_nodes(true);
    actions.push(
        DisconnectPeers {
            targets: vec!["db0".into(), "db2".into()],
            common: on("db1"),
            ..Default::default()
        }
        .into(),
    );
    actions.extend(node("db3", "peer3"));
    actions.extend([
        watch("db3"),
        ConnectPeers {
            target: "db1".into(),
            common: on("db3"),
            ..Default::default()
        }
        .into(),
        ConnectPeers {
            target: "db0".into(),
            common: on("db3"),
            ..Default::default()
        }
        .into(),
        // db3's list names db1 to db0; db0's names db2 to db3, which dials
        // it and names db1 to db2.
        learned("db0", &["db3"]),
        learned("db3", &["db0"]),
        learned("db2", &["db3"]),
        ActivePeers {
            expected_data: json!({ "addresses": m::unordered(json!([m::r("peer0"), m::r("peer1"), m::r("peer2")])) }),
            common: on("db3"),
            ..Default::default()
        }
        .into(),
        ActivePeers {
            expected_data: json!({ "addresses": m::unordered(json!([m::r("peer3")])) }),
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
    let mut actions = three_connected_nodes(true);
    actions.extend([
        // db2 holds db0, so its fresh list names it.
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
        learned("db1", &["db2"]),
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
