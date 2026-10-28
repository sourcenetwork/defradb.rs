//! Generated from the conformance scenarios of tests/integration/acp/nac/relation_admin/sync_p2p_branchable_collection_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, AddNACActorRelationship, ConnectPeers, GetCollectionsFiltered,
    NodeOpenP2P, NodeStopForRestart, PeerEventsStart, Start, SyncBranchableCollection,
    WaitForUpdates,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.nac.admin.p2p")]
fn nac_admin_relation_can_sync_p2_p_branchable_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2P {
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NodeStopForRestart {
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NodeStopForRestart {
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(1),
                common: Common { node: Some("db1".into()), identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(0),
                common: Common { node: Some("db0".into()), identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db1".into(),
                common: Common { node: Some("db0".into()), identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            GetCollectionsFiltered {
                collection_name: Some("User".into()),
                common: Common { node: Some("db0".into()), identity_seed: Some(2), capture: captures([("collection0", "/collections/0/CollectionID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { node: Some("db0".into()), identity_seed: Some(2), retry: true, capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc0"), Text::reference("collection0")],
                common: Common { node: Some("db0".into()), capture: captures([("head0", "/cids/0")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["User".into()],
                collection_index: 0,
                expected_error: Some("not authorized to perform operation. Permission: sync-p2p-branchable-collection".into()),
                common: Common { node: Some("db1".into()), identity_seed: Some(3), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddNACActorRelationship {
                target_identity_seed: Some(3),
                relation: "admin".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common { node: Some("db0".into()), identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddNACActorRelationship {
                target_identity_seed: Some(3),
                relation: "admin".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common { node: Some("db1".into()), identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["User".into()],
                collection_index: 0,
                common: Common { node: Some("db1".into()), identity_seed: Some(3), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
