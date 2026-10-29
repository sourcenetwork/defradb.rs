//! Generated from the conformance scenarios of tests/integration/collection_version/migrations/query/with_p2p_collection_branch_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, AddReplicator, ConfigureMigration, NewNode, PatchCollection, Request,
    WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.query.p2p")]
fn collection_migration_query_with_p2_p_replicated_doc_on_other_collection_branch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        verified: Boolean
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        verified: Boolean
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": "String"} }
					]
				"#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: "bafyreiabmrtgxy5dgotuc53gfaamuqhlzugyeetzbuv7s3x6ufmlr5ylga".into(),
                destination_version_id: "bafyreidwvvr7kp5rqt7dbgzw55vuueovkjz6b2mlvz3rq2pxf22fqenzdm".into(),
                lens: Lens {
                    lenses: Some(vec![
                        LensModule {
                            arguments: json!({ "dst": "name", "value": "Fred" }),
                            path: "bin/lenses/set-default.wasm".into(),
                        },
                    ]),
                },
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: "bafyreiabmrtgxy5dgotuc53gfaamuqhlzugyeetzbuv7s3x6ufmlr5ylga".into(),
                destination_version_id: "bafyreidwvvr7kp5rqt7dbgzw55vuueovkjz6b2mlvz3rq2pxf22fqenzdm".into(),
                lens: Lens {
                    lenses: Some(vec![
                        LensModule {
                            arguments: json!({ "dst": "name", "value": "Fred" }),
                            path: "bin/lenses/set-default.wasm".into(),
                        },
                    ]),
                },
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "phone", "Kind": 11} }
					]
				"#.into(),
                lens: Some(Lens {
                    lenses: Some(vec![
                        LensModule {
                            arguments: json!({ "dst": "phone", "value": "1234567890" }),
                            path: "bin/lenses/set-default.wasm".into(),
                        },
                    ]),
                }),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "verified": true })),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                common: Common { node: Some("db0".into()), capture: captures([("head", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                cid: Some(Text::reference("head")),
                expected_data: json!({}),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            verified
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": m::unordered(json!([{ "name": "John", "verified": true }])) }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            name
                            phone
                            verified
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "Users": m::unordered(json!([{ "name": null, "phone": "1234567890", "verified": true }])),
                }),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
