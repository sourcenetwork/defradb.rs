//! A hub started with pinc's relay role (`defradb hub --relay`) lets its
//! members reach each other by peer id alone, as nodes behind NATs that know
//! no address of each other must. A hub without the role carries topics only.

use integration_test::actions::{Actions, ActivePeers, ConnectPeers, NewNode, PeerInfo};
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

fn node(name: &str, peer: &str, relay_hub: bool) -> [Actions; 2] {
    [
        NewNode {
            network: true,
            relay_hub,
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

fn by_id(expected_error: Option<&str>) -> Actions {
    ConnectPeers {
        target: "db0".into(),
        by_id: true,
        expected_error: expected_error.map(Into::into),
        common: on("db1"),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.cli.hub.relay")]
fn members_of_a_relaying_hub_connect_by_peer_id_alone() {
    let mut actions: Vec<Actions> = [node("db0", "peer0", true), node("db1", "peer1", true)]
        .into_iter()
        .flatten()
        .collect();
    actions.extend([
        by_id(None),
        // db1 is connected to db0 and nobody else. A relayed session may
        // have no direct address yet, so the entry is checked to be there,
        // not for an address.
        ActivePeers {
            expected_data: json!({ "addresses": [m::nonempty_string()] }),
            common: on("db1"),
            ..Default::default()
        }
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cli.hub.relay")]
fn members_of_a_hub_that_does_not_relay_cannot_connect_by_peer_id_alone() {
    let mut actions: Vec<Actions> = [node("db0", "peer0", false), node("db1", "peer1", false)]
        .into_iter()
        .flatten()
        .collect();
    actions.push(by_id(Some("failed to connect: timeout dialing")));
    TestCase::from_actions(actions).execute();
}
