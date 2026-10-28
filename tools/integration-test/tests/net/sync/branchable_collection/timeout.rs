//! Generated from the conformance scenarios of tests/integration/net/sync/branchable_collection/timeout_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, ConnectPeers, GetCollectionsFiltered, NodeOpenP2PC, PeerEventsStart,
    Request, SyncBranchableCollection, WaitForUpdates,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.net.sync.branchable_deadline")]
fn branchable_collection_sync_finite_deadline_context_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2PC {
                node_identity_seed: 0,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeOpenP2PC {
                node_identity_seed: 1,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db1".into()),
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
            GetCollectionsFiltered {
                collection_name: Some("User".into()),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("collection", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc"), Text::reference("collection")],
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("collectionHead", "/cids/1"), ("docHead", "/cids/0")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db1".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["User".into()],
                collection_index: 0,
                no_deadline: Some(false),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
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
#[mantra_macros::req_test("go.net.sync.branchable_deadline")]
fn branchable_collection_sync_no_deadline_context_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2PC {
                node_identity_seed: 0,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeOpenP2PC {
                node_identity_seed: 1,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db1".into()),
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
            GetCollectionsFiltered {
                collection_name: Some("User".into()),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("collection", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc"), Text::reference("collection")],
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("collectionHead", "/cids/1"), ("docHead", "/cids/0")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db1".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["User".into()],
                collection_index: 0,
                no_deadline: Some(true),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
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
