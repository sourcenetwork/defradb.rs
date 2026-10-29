//! Generated from the conformance scenarios of tests/integration/net/sync/collection_version/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, ConnectPeers, GetCollections, NewNode, Request,
    SetActiveCollectionVersion, SyncCollectionVersions, SyncDocs, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.net.sync.collection_version")]
fn sync_col_version_with_initial_col_version_can_be_activated_and_queried() {
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
            NewNode {
                network: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
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
            GetCollections {
                collection_name: Some("Users".into()),
                filtered: true,
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("version0", "/collections/0/VersionID")]),
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
            SyncCollectionVersions {
                version_ids: vec![Text::reference("version0")],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: Text::reference("version0"),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 4, "name": "John" })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SyncDocs {
                collection: "Users".into(),
                doc_ids: vec![Text::reference("doc0_0")],
                concurrency: 1,
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
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
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
#[mantra_macros::req_test("go.net.sync.collection_version")]
fn sync_col_version_with_initial_col_version() {
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
            NewNode {
                network: true,
                common: Common {
                    node: Some("db1".into()),
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
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Users".into()),
                filtered: true,
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("version0", "/collections/0/VersionID")]),
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
            SyncCollectionVersions {
                version_ids: vec![
                    "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu".into(),
                ],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Users",
                            "PreviousVersion": null,
                            "Fields": [
                                m::object(json!({
                                    "Name": "_docID",
                                    "Kind": 1,
                                    "Typ": 0,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "name",
                                    "Kind": 11,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                            ],
                        })),
                    ],
                }),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                get_inactive: Some(true),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": false,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Users",
                            "PreviousVersion": null,
                            "Fields": [
                                m::object(json!({
                                    "Name": "_docID",
                                    "Kind": 1,
                                    "Typ": 0,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "name",
                                    "Kind": 11,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                            ],
                        })),
                    ],
                }),
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
