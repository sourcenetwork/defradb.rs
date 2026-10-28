//! Generated from the conformance scenarios of tests/integration/acp/nac/disconnect_p2p_peer_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    ActivePeers, ConnectPeers, DisconnectPeers, NodeOpenP2P, NodeStopForRestart, PeerInfo, Start,
};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.nac.gates.p2p-peers")]
fn nac_gates_disconnect_p2_p_peer_authorized_identity_allow_access() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2P {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeStopForRestart {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeStopForRestart {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(1),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(0),
                common: Common {
                    node: Some("db0".into()),
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerInfo {
                common: Common {
                    node: Some("db0".into()),
                    identity_seed: Some(0),
                    capture: captures([("peer0", "/addresses/0")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ActivePeers {
                expected_data: json!({ "addresses": m::unordered(json!([m::r("peer0")])) }),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DisconnectPeers {
                targets: vec!["db0".into()],
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ActivePeers {
                expected_data: json!({ "addresses": m::unordered(json!([])) }),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(2),
                    ..Default::default()
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

#[test]
#[mantra_macros::req_test("go.acp.nac.gates.p2p-peers")]
fn nac_gates_disconnect_p2_p_peer_no_identity_not_authorized_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2P {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeStopForRestart {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeStopForRestart {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(1),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(0),
                common: Common {
                    node: Some("db0".into()),
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DisconnectPeers {
                targets: vec!["db0".into()],
                expected_error: Some(
                    "not authorized to perform operation. Permission: disconnect-p2p-peer".into(),
                ),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
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

#[test]
#[mantra_macros::req_test("go.acp.nac.gates.p2p-peers")]
fn nac_gates_disconnect_p2_p_peer_wrong_identity_not_authorized_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2P {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeStopForRestart {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeStopForRestart {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(1),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(0),
                common: Common {
                    node: Some("db0".into()),
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DisconnectPeers {
                targets: vec!["db0".into()],
                expected_error: Some(
                    "not authorized to perform operation. Permission: disconnect-p2p-peer".into(),
                ),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(3),
                    ..Default::default()
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
