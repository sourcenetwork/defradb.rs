//! Generated from the conformance scenarios of tests/integration/net/sync/branchable_collection/complex_network_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{EventType, Topic};
use integration_test::actions::{
    AddCollection, AddDoc, ConnectPeers, GetCollections, NewNode, Request,
    SyncBranchableCollection, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.net.sync.branchable_dag")]
fn branchable_collection_sync_with_documents_from_peers_and_new_head_after_sync_should_have_identical_dag(
) {
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
            NewNode {
                network: true,
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common { node: Some("db3".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
                    }
                "#.into(),
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
                    }
                "#.into(),
                common: Common { node: Some("db3".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("User".into()),
                filtered: true,
                common: Common { node: Some("db0".into()), capture: captures([("collection0", "/collections/0/CollectionID")]), ..Default::default() },
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
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db3".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "origin": "node0" })),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![Text::reference("doc0_0"), Text::reference("collection0")]),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "origin": "node1" })),
                common: Common { node: Some("db1".into()), retry: true, capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![Text::reference("doc0_1"), Text::reference("collection0")]),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "origin": "node2" })),
                common: Common { node: Some("db2".into()), retry: true, capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![Text::reference("doc0_2"), Text::reference("collection0")]),
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "origin": "node3" })),
                common: Common { node: Some("db3".into()), retry: true, capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![Text::reference("doc0_3"), Text::reference("collection0")]),
                common: Common { node: Some("db3".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db1".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db3".into(),
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: Some(vec![
                    Topic {
                        topic: "sync-branchable".into(),
                        peers: vec!["db1".into()],
                    },
                ]),
                event_type: Some(EventType::Joined),
                millis: Some(5000),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: Some(vec![
                    Topic {
                        topic: "sync-branchable".into(),
                        peers: vec!["db3".into()],
                    },
                ]),
                event_type: Some(EventType::Joined),
                millis: Some(5000),
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["User".into()],
                collection_index: 0,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["User".into()],
                collection_index: 0,
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db2".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db3".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db2".into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db3".into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: Some(vec![
                    Topic {
                        topic: "sync-branchable".into(),
                        peers: vec!["db2".into(), "db3".into()],
                    },
                ]),
                event_type: Some(EventType::Joined),
                millis: Some(5000),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: Some(vec![
                    Topic {
                        topic: "sync-branchable".into(),
                        peers: vec!["db2".into(), "db3".into()],
                    },
                ]),
                event_type: Some(EventType::Joined),
                millis: Some(5000),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["User".into()],
                collection_index: 0,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["User".into()],
                collection_index: 0,
                common: Common { node: Some("db3".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["User".into()],
                collection_index: 0,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["User".into()],
                collection_index: 0,
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "origin": "all" })),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![Text::reference("doc0_4"), Text::reference("collection0")]),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "origin": "all" })),
                common: Common { node: Some("db1".into()), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![Text::reference("doc0_4"), Text::reference("collection0")]),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "origin": "all" })),
                common: Common { node: Some("db2".into()), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![Text::reference("doc0_4"), Text::reference("collection0")]),
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno", "origin": "all" })),
                common: Common { node: Some("db3".into()), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![Text::reference("doc0_4"), Text::reference("collection0")]),
                common: Common { node: Some("db3".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        _commits(filter: {fieldName: {_eq: null}}) {
                                            cid
                                            height
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::same("sameCid1"), "height": 2 },
                        { "cid": m::same("sameCid2"), "height": 1 },
                        { "cid": m::same("sameCid3"), "height": 1 },
                        { "cid": m::same("sameCid4"), "height": 1 },
                        { "cid": m::same("sameCid5"), "height": 1 },
                    ],
                }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        _commits(filter: {fieldName: {_eq: null}}) {
                                            cid
                                            height
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::same("sameCid1"), "height": 2 },
                        { "cid": m::same("sameCid2"), "height": 1 },
                        { "cid": m::same("sameCid3"), "height": 1 },
                        { "cid": m::same("sameCid4"), "height": 1 },
                        { "cid": m::same("sameCid5"), "height": 1 },
                    ],
                }),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        _commits(filter: {fieldName: {_eq: null}}) {
                                            cid
                                            height
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::same("sameCid1"), "height": 2 },
                        { "cid": m::same("sameCid2"), "height": 1 },
                        { "cid": m::same("sameCid3"), "height": 1 },
                        { "cid": m::same("sameCid4"), "height": 1 },
                        { "cid": m::same("sameCid5"), "height": 1 },
                    ],
                }),
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        _commits(filter: {fieldName: {_eq: null}}) {
                                            cid
                                            height
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::same("sameCid1"), "height": 2 },
                        { "cid": m::same("sameCid2"), "height": 1 },
                        { "cid": m::same("sameCid3"), "height": 1 },
                        { "cid": m::same("sameCid4"), "height": 1 },
                        { "cid": m::same("sameCid5"), "height": 1 },
                    ],
                }),
                common: Common { node: Some("db3".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        _commits(filter: {fieldName: {_eq: null}}, order: {height: DESC}, limit: 1) {
                                            heads {
                                                fieldName
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "_commits": [{ "heads": m::length(4) }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        _commits(filter: {fieldName: {_eq: null}}, order: {height: DESC}, limit: 1) {
                                            heads {
                                                fieldName
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "_commits": [{ "heads": m::length(4) }] }),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        _commits(filter: {fieldName: {_eq: null}}, order: {height: DESC}, limit: 1) {
                                            heads {
                                                fieldName
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "_commits": [{ "heads": m::length(4) }] }),
                common: Common { node: Some("db2".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        _commits(filter: {fieldName: {_eq: null}}, order: {height: DESC}, limit: 1) {
                                            heads {
                                                fieldName
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "_commits": [{ "heads": m::length(4) }] }),
                common: Common { node: Some("db3".into()), ..Default::default() },
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
#[mantra_macros::req_test("go.net.sync.branchable_dag")]
fn branchable_collection_sync_with_documents_from_peers_should_have_identical_dag() {
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
            NewNode {
                network: true,
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common {
                    node: Some("db3".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
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
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
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
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db3".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("User".into()),
                filtered: true,
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
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
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db3".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "origin": "node0" })),
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
                doc_ids: Some(vec![
                    Text::reference("doc0_0"),
                    Text::reference("collection0"),
                ]),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "origin": "node1" })),
                common: Common {
                    node: Some("db1".into()),
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_1"),
                    Text::reference("collection0"),
                ]),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "origin": "node2" })),
                common: Common {
                    node: Some("db2".into()),
                    retry: true,
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_2"),
                    Text::reference("collection0"),
                ]),
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "origin": "node3" })),
                common: Common {
                    node: Some("db3".into()),
                    retry: true,
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_3"),
                    Text::reference("collection0"),
                ]),
                common: Common {
                    node: Some("db3".into()),
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
            ConnectPeers {
                target: "db2".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db3".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db2".into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db3".into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db3".into(),
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: Some(vec![Topic {
                    topic: "sync-branchable".into(),
                    peers: vec!["db1".into(), "db2".into(), "db3".into()],
                }]),
                event_type: Some(EventType::Joined),
                millis: Some(5000),
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
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: Some(vec![Topic {
                    topic: "sync-branchable".into(),
                    peers: vec!["db0".into(), "db2".into(), "db3".into()],
                }]),
                event_type: Some(EventType::Joined),
                millis: Some(5000),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["User".into()],
                collection_index: 0,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: Some(vec![Topic {
                    topic: "sync-branchable".into(),
                    peers: vec!["db0".into(), "db1".into(), "db3".into()],
                }]),
                event_type: Some(EventType::Joined),
                millis: Some(5000),
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["User".into()],
                collection_index: 0,
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: Some(vec![Topic {
                    topic: "sync-branchable".into(),
                    peers: vec!["db0".into(), "db1".into(), "db2".into()],
                }]),
                event_type: Some(EventType::Joined),
                millis: Some(5000),
                common: Common {
                    node: Some("db3".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["User".into()],
                collection_index: 0,
                common: Common {
                    node: Some("db3".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        _commits(filter: {fieldName: {_eq: null}}) {
                                            cid
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::same("sameCid1") },
                        { "cid": m::same("sameCid2") },
                        { "cid": m::same("sameCid3") },
                        { "cid": m::same("sameCid4") },
                    ],
                }),
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
                                        _commits(filter: {fieldName: {_eq: null}}) {
                                            cid
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::same("sameCid1") },
                        { "cid": m::same("sameCid2") },
                        { "cid": m::same("sameCid3") },
                        { "cid": m::same("sameCid4") },
                    ],
                }),
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
                                        _commits(filter: {fieldName: {_eq: null}}) {
                                            cid
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::same("sameCid1") },
                        { "cid": m::same("sameCid2") },
                        { "cid": m::same("sameCid3") },
                        { "cid": m::same("sameCid4") },
                    ],
                }),
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        _commits(filter: {fieldName: {_eq: null}}) {
                                            cid
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::same("sameCid1") },
                        { "cid": m::same("sameCid2") },
                        { "cid": m::same("sameCid3") },
                        { "cid": m::same("sameCid4") },
                    ],
                }),
                common: Common {
                    node: Some("db3".into()),
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
#[mantra_macros::req_test("go.net.sync.branchable_copy")]
fn branchable_collection_sync_with_multiple_docs_in_complex_linked_network_should_sync_all() {
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
            NewNode {
                network: true,
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common {
                    node: Some("db3".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common {
                    node: Some("db4".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
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
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
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
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db3".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db4".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("User".into()),
                filtered: true,
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
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
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db3".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db4".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "origin": "node0" })),
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
                doc_ids: Some(vec![
                    Text::reference("doc0_0"),
                    Text::reference("collection0"),
                ]),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "origin": "node1" })),
                common: Common {
                    node: Some("db1".into()),
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_1"),
                    Text::reference("collection0"),
                ]),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "origin": "node2" })),
                common: Common {
                    node: Some("db2".into()),
                    retry: true,
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_2"),
                    Text::reference("collection0"),
                ]),
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "origin": "node3" })),
                common: Common {
                    node: Some("db3".into()),
                    retry: true,
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_3"),
                    Text::reference("collection0"),
                ]),
                common: Common {
                    node: Some("db3".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "origin": "node4" })),
                common: Common {
                    node: Some("db4".into()),
                    retry: true,
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_4"),
                    Text::reference("collection0"),
                ]),
                common: Common {
                    node: Some("db4".into()),
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
            ConnectPeers {
                target: "db2".into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db3".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db4".into(),
                common: Common {
                    node: Some("db3".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: Some(vec![Topic {
                    topic: "sync-branchable".into(),
                    peers: vec!["db1".into(), "db2".into(), "db3".into(), "db4".into()],
                }]),
                event_type: Some(EventType::Joined),
                millis: Some(5000),
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
                                        User {
                                            name
                                            origin
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": m::unordered(json!([
                        { "name": "John", "origin": "node0" },
                        { "name": "Islam", "origin": "node1" },
                        { "name": "Fred", "origin": "node2" },
                        { "name": "Shahzad", "origin": "node3" },
                        { "name": "Andy", "origin": "node4" },
                    ])),
                }),
                common: Common {
                    node: Some("db0".into()),
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
#[mantra_macros::req_test("go.net.sync.branchable_copy")]
fn branchable_collection_sync_with_multiple_document_heads_received_from_peers_should_sync_all() {
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
            NewNode {
                network: true,
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
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
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
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
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                        origin: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("User".into()),
                filtered: true,
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
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
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "origin": "node1" })),
                common: Common {
                    node: Some("db1".into()),
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_0"),
                    Text::reference("collection0"),
                ]),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "origin": "node2" })),
                common: Common {
                    node: Some("db2".into()),
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_1"),
                    Text::reference("collection0"),
                ]),
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db2".into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            SyncBranchableCollection {
                collections: vec!["User".into()],
                collection_index: 0,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "origin": "node0" })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_2"),
                    Text::reference("collection0"),
                ]),
                common: Common {
                    node: Some("db0".into()),
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
                                        User {
                                            name
                                            origin
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": m::unordered(json!([
                        { "name": "John", "origin": "node0" },
                        { "name": "Islam", "origin": "node1" },
                        { "name": "Fred", "origin": "node2" },
                    ])),
                }),
                common: Common {
                    node: Some("db0".into()),
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
