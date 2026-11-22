use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddCollectionSubscription, AddDoc, AddReplicator, ConfigureMigration,
    ConnectPeers, GetCollections, NewNode, PatchCollection, Request, SyncDocs, UpdateDoc,
    WaitForIndexReady, WaitForPeersEvents, WaitForSync,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.net.migrated_counter_merge")]
fn an_old_version_counter_update_merges_through_a_lens_once() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common {
                    node: Some("peer".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type U {
                        name: String
                        n: Int @crdt(type: pcounter) @index
                    }
                "#
                .into(),
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type U {
                        name: String
                        n: Int @crdt(type: pcounter) @index
                    }
                "#
                .into(),
                common: Common {
                    node: Some("peer".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("U".into()),
                common: Common {
                    node: Some("migrated".into()),
                    capture: captures([("source_version", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("peer".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "migrated".into(),
                common: Common {
                    node: Some("peer".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "name": "counter", "n": 1 })),
                common: Common {
                    node: Some("peer".into()),
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                common: Common {
                    node: Some("peer".into()),
                    capture: captures([("head", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "peer".into(),
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["U".into()],
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SyncDocs {
                collection: "U".into(),
                doc_ids: vec![Text::reference("doc")],
                concurrency: 1,
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                cid: Some(Text::reference("head")),
                expected_data: json!({}),
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
                    [
                        { "op": "add", "path": "/U/Fields/-", "value": {"Name": "note", "Kind": "String"} }
                    ]
                "#
                .into(),
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("U".into()),
                common: Common {
                    node: Some("migrated".into()),
                    capture: captures([("active_version", "/collections/0/VersionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConfigureMigration {
                source_version_id: Text::reference("source_version"),
                destination_version_id: Text::reference("active_version"),
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "field": "n", "value": 1 }),
                        path: "bin/lenses/increment.wasm".into(),
                    }]),
                },
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForIndexReady {
                collection: Some("U".into()),
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        U {
                            n
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "U": [{ "n": 2 }] }),
                common: Common {
                    node: Some("migrated".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "U".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "n": 1 }),
                common: Common {
                    node: Some("peer".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                common: Common {
                    node: Some("peer".into()),
                    capture: captures([("head1", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        U {
                            n
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "U": [{ "n": 2 }] }),
                common: Common {
                    node: Some("peer".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SyncDocs {
                collection: "U".into(),
                doc_ids: vec![Text::reference("doc")],
                concurrency: 1,
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                cid: Some(Text::reference("head1")),
                expected_data: json!({}),
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        U {
                            n
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "U": [{ "n": 3 }] }),
                common: Common {
                    node: Some("migrated".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "U".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "n": 1 }),
                common: Common {
                    node: Some("peer".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                common: Common {
                    node: Some("peer".into()),
                    capture: captures([("head2", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        U {
                            n
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "U": [{ "n": 3 }] }),
                common: Common {
                    node: Some("peer".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SyncDocs {
                collection: "U".into(),
                doc_ids: vec![Text::reference("doc")],
                concurrency: 1,
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                cid: Some(Text::reference("head2")),
                expected_data: json!({}),
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        U {
                            n
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "U": [{ "n": 4 }] }),
                common: Common {
                    node: Some("migrated".into()),
                    retry: true,
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
