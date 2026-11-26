//! A node's bootstrap peers (`net.peers`) are dialed as the node, so node
//! access control, which gates a caller's `connect`, does not refuse them.

use integration_test::actions::{ActivePeers, Close, NewNode, PeerInfo, Start};
use integration_test::m;
use integration_test::scenario::{captures, Common, Eventually};
use integration_test::test_case::TestCase;
use serde_json::json;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

/// db1 starts with node access control enabled and db0 as its bootstrap
/// peer: its start dials db0 with no caller's identity, and the dial
/// connects rather than being refused for the wildcard identity.
#[test]
#[mantra_macros::req_test("defra.cli.pinc.bootstrap_nac")]
fn a_node_with_access_control_dials_its_bootstrap_peers() {
    TestCase::from_actions(vec![
        NewNode {
            network: true,
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        PeerInfo {
            common: Common {
                capture: captures([("peer0", "/addresses/0")]),
                ..on("db0")
            },
            ..Default::default()
        }
        .into(),
        NewNode {
            network: true,
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        Close {
            keep_data: true,
            common: on("db1"),
            ..Default::default()
        }
        .into(),
        Start {
            enable_nac: true,
            node_identity_seed: Some(1),
            bootstrap: vec!["db0".into()],
            common: Common {
                identity_seed: Some(2),
                ..on("db1")
            },
            ..Default::default()
        }
        .into(),
        ActivePeers {
            expected_data: json!({ "addresses": m::unordered(json!([m::r("peer0")])) }),
            common: Common {
                identity_seed: Some(2),
                eventually: Some(Eventually {
                    timeout_millis: Some(15_000),
                    ..Default::default()
                }),
                ..on("db1")
            },
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
