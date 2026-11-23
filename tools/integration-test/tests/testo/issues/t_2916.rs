use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddCollectionSubscription, AddDoc, AddReplicator, ConfigureMigration,
    ConnectPeers, GetCollections, NewNode, PatchCollection, Request, SyncDocs, UpdateDoc,
    WaitForIndexReady, WaitForPeersEvents, WaitForSync,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

/// A peer at the source version increments `n` twice after `migrated` has
/// moved `U` to a new version through `lenses`. `migrated` answers `query`
/// with `expected` after the migration and after each increment.
fn old_version_counter_updates(
    id: &'static str,
    sdl: &str,
    lenses: Vec<LensModule>,
    query: &str,
    expected: [serde_json::Value; 3],
) {
    let [migrated, first, second] = expected;
    TestCase {
        id,
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
                sdl: sdl.into(),
                common: Common {
                    node: Some("migrated".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: sdl.into(),
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
                    lenses: Some(lenses),
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
                request: query.into(),
                expected_data: json!({ "U": [migrated] }),
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
                request: query.into(),
                expected_data: json!({ "U": [first] }),
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
                request: query.into(),
                expected_data: json!({ "U": [second] }),
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

const INDEXED: &str = r#"
    type U {
        name: String
        n: Int @crdt(type: pcounter) @index
    }
"#;

const UNINDEXED: &str = r#"
    type U {
        name: String
        n: Int @crdt(type: pcounter)
    }
"#;

const COUNTER: &str = "query { U { n } }";
const COUNTER_AND_NOTE: &str = "query { U { n note } }";

fn increment() -> LensModule {
    LensModule {
        arguments: json!({ "field": "n", "value": 1 }),
        path: "bin/lenses/increment.wasm".into(),
    }
}

fn copy_name_to_note() -> LensModule {
    LensModule {
        arguments: json!({ "src": "name", "dst": "note" }),
        path: "bin/lenses/copy.wasm".into(),
    }
}

fn default_note() -> LensModule {
    LensModule {
        arguments: json!({ "dst": "note", "value": "set" }),
        path: "bin/lenses/set_default.wasm".into(),
    }
}

#[test]
#[mantra_macros::req_test("defra.net.migrated_counter_merge")]
fn an_old_version_counter_update_merges_through_a_lens_once() {
    old_version_counter_updates(
        integration_test::scenario_id!(),
        INDEXED,
        vec![increment()],
        COUNTER,
        [json!({ "n": 2 }), json!({ "n": 3 }), json!({ "n": 4 })],
    );
}

#[test]
#[mantra_macros::req_test("defra.net.migrated_counter_merge")]
fn an_old_version_counter_update_merges_through_a_forward_only_lens() {
    old_version_counter_updates(
        integration_test::scenario_id!(),
        INDEXED,
        vec![copy_name_to_note()],
        COUNTER_AND_NOTE,
        [
            json!({ "n": 1, "note": "counter" }),
            json!({ "n": 2, "note": "counter" }),
            json!({ "n": 3, "note": "counter" }),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.net.migrated_counter_merge")]
fn an_old_version_counter_update_adds_to_a_counter_a_forward_only_lens_wrote() {
    old_version_counter_updates(
        integration_test::scenario_id!(),
        INDEXED,
        vec![increment(), copy_name_to_note()],
        COUNTER_AND_NOTE,
        [
            json!({ "n": 2, "note": "counter" }),
            json!({ "n": 3, "note": "counter" }),
            json!({ "n": 4, "note": "counter" }),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.net.migrated_counter_merge")]
fn an_old_version_counter_update_keeps_the_fields_the_lens_wrote() {
    old_version_counter_updates(
        integration_test::scenario_id!(),
        INDEXED,
        vec![increment(), default_note()],
        COUNTER_AND_NOTE,
        [
            json!({ "n": 2, "note": "set" }),
            json!({ "n": 3, "note": "set" }),
            json!({ "n": 4, "note": "set" }),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.net.migrated_counter_merge")]
fn an_old_version_counter_update_lands_on_a_document_stored_at_the_source_version() {
    old_version_counter_updates(
        integration_test::scenario_id!(),
        UNINDEXED,
        vec![increment(), default_note()],
        COUNTER_AND_NOTE,
        [
            json!({ "n": 2, "note": "set" }),
            json!({ "n": 3, "note": "set" }),
            json!({ "n": 4, "note": "set" }),
        ],
    );
}
