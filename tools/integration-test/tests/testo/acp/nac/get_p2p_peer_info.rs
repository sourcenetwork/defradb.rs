//! Node access control gates a node's own peer information, as Go's
//! `tests/integration/acp/nac/get_p2p_peer_info_test.go` does.

use integration_test::actions::{Close, NewNode, PeerInfo, Start};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;

/// The NAC owner reads the node's peer information.
#[test]
#[mantra_macros::req_test("go.acp.nac.gates.p2p-peers")]
fn nac_gates_get_p2_p_peer_info_authorized_identity_allow_access() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Close {
                keep_data: true,
                common: Common {
                    node: Some("db0".into()),
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
            PeerInfo {
                common: Common {
                    node: Some("db0".into()),
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

/// A caller with no identity is refused.
#[test]
#[mantra_macros::req_test("go.acp.nac.gates.p2p-peers")]
fn nac_gates_get_p2_p_peer_info_no_identity_not_authorized_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Close {
                keep_data: true,
                common: Common {
                    node: Some("db0".into()),
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
            PeerInfo {
                expected_error: Some(
                    "not authorized to perform operation. Permission: get-p2p-peer-info".into(),
                ),
                common: Common {
                    node: Some("db0".into()),
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

/// An identity without the permission is refused.
#[test]
#[mantra_macros::req_test("go.acp.nac.gates.p2p-peers")]
fn nac_gates_get_p2_p_peer_info_wrong_identity_not_authorized_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Close {
                keep_data: true,
                common: Common {
                    node: Some("db0".into()),
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
            PeerInfo {
                expected_error: Some(
                    "not authorized to perform operation. Permission: get-p2p-peer-info".into(),
                ),
                common: Common {
                    node: Some("db0".into()),
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
