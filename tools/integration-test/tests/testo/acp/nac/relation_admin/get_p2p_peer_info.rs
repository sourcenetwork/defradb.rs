//! A granted non-owner reads a node's peer information, after Go's
//! `tests/integration/acp/nac/relation_admin/get_p2p_peer_info_test.go`.

use integration_test::actions::{AddNACActorRelationship, Close, NewNode, PeerInfo, Start};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

/// Refused before the `admin` grant, allowed after it.
#[test]
#[mantra_macros::req_test("go.acp.nac.admin.p2p")]
fn nac_admin_relation_can_get_p2_p_peer_info() {
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
            AddNACActorRelationship {
                target_identity_seed: Some(3),
                relation: "admin".into(),
                expected_data: json!({ "existedAlready": false }),
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
