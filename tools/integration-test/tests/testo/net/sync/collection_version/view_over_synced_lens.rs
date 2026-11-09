//! A lens that reaches a node by P2P sync is the node's own: a view there can
//! name it, before and after a restart.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddLens, AddView, ConnectPeers, GetCollections, NewNode, Restart,
    SyncCollectionVersions,
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

fn view_on_db1(name: &str) -> AddView {
    AddView {
        sdl: format!("type {name} @materialized(if: false) {{ fullName: String }}").into(),
        query: "Users { name }".into(),
        transform_cid: Some(Text::reference("lens0")),
        common: on("db1"),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.net.sync.lens_is_kept")]
fn a_view_names_a_lens_p2p_sync_fetched_before_and_after_a_restart() {
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
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "fullName", "src": "name" }),
                        path: "bin/lenses/copy.wasm".into(),
                    }]),
                },
                common: Common {
                    capture: captures([("lens0", "/lensID")]),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            AddView {
                sdl: "type UserView @materialized(if: false) { fullName: String }".into(),
                query: "Users { name }".into(),
                transform_cid: Some(Text::reference("lens0")),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("UserView".into()),
                filtered: true,
                common: Common {
                    capture: captures([("version1", "/collections/0/VersionID")]),
                    ..on("db0")
                },
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
                version_ids: vec![Text::reference("version1")],
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            view_on_db1("SyncedView").into(),
            Restart {
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            view_on_db1("RestartedView").into(),
        ],
        ..Default::default()
    }
    .execute();
}
