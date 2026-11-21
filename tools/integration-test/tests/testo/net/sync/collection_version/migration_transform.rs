//! A migration transform that reaches a node by P2P sync stays attached to its
//! synced definition.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, AddReplicator, ConfigureMigration, ConnectPeers, GetCollections,
    NewNode, PatchCollection, Request, SyncCollectionVersions, WaitForPeersEvents, WaitForSync,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.net.sync.migration_transform_is_kept")]
fn a_synced_migration_transform_stays_attached_to_its_definition() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Users { name: String }".into(),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Users { name: String }".into(),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0", "/docID"), ("head0", "/cid")]),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                filtered: true,
                common: Common {
                    capture: captures([("version1", "/collections/0/VersionID")]),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
                    [
                        { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "nickname", "Kind": "String"} }
                    ]
                "#
                .into(),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            GetCollections {
                filtered: true,
                common: Common {
                    capture: captures([("version2", "/collections/0/VersionID")]),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: Text::reference("version1"),
                destination_version_id: Text::reference("version2"),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "nickname", "src": "name" }),
                        path: "bin/lenses/copy.wasm".into(),
                    }]),
                },
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db1".into(),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            SyncCollectionVersions {
                version_ids: vec![Text::reference("version1"), Text::reference("version2")],
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                cid: Some(Text::reference("head0")),
                expected_data: json!({}),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            name
                            nickname
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{
                        "name": "John",
                        "nickname": "John",
                    }],
                }),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
