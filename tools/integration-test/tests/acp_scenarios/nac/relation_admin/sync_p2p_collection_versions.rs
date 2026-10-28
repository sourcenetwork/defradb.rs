//! Generated from the conformance scenarios of tests/integration/acp/nac/relation_admin/sync_p2p_collection_versions_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddNACActorRelationship, NodeOpenP2P, NodeStopForRestart, PeerEventsStart,
    Start, SyncCollectionVersions,
};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.nac.admin.p2p")]
fn nac_admin_relation_can_sync_p2_p_collection_versions() {
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
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SyncCollectionVersions {
                version_ids: vec![
                    "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu".into(),
                ],
                expected_error: Some(
                    "not authorized to perform operation. Permission: sync-p2p-collection-versions"
                        .into(),
                ),
                common: Common {
                    node: Some("db1".into()),
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
            AddNACActorRelationship {
                target_identity_seed: Some(3),
                relation: "admin".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SyncCollectionVersions {
                version_ids: vec![
                    "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu".into(),
                ],
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
