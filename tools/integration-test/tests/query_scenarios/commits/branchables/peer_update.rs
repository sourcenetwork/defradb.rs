//! Generated from the conformance scenarios of tests/integration/query/commits/branchables/peer_update_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddCollectionSubscription, AddDoc, ConnectPeers, GetCollectionsFiltered,
    NodeOpenP2P, PeerEventsStart, Request, UpdateDoc, WaitForMerge, WaitForUpdates,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.branchable.sync")]
fn query_commits_branchables_handles_concurrent_updates_across_peer_connection() {
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
            AddCollection {
                sdl: r#"
                    type Users @branchable {
                        name: String
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users @branchable {
                        name: String
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            GetCollectionsFiltered {
                collection_name: Some("Users".into()),
                common: Common { node: Some("db0".into()), capture: captures([("collection0", "/collections/0/CollectionID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc0_0"), Text::reference("collection0")],
                common: Common { node: Some("db0".into()), capture: captures([("collectionHead0", "/cids/1"), ("head0", "/cids/0")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { node: Some("db1".into()), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc0_0"), Text::reference("collection0")],
                common: Common { node: Some("db1".into()), capture: captures([("collectionHead1", "/cids/1"), ("head1", "/cids/0")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Fred" }),
                expected_data: json!({}),
                common: Common { node: Some("db0".into()), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc0_0"), Text::reference("collection0")],
                common: Common { node: Some("db0".into()), capture: captures([("collectionHead2", "/cids/1"), ("head2", "/cids/0")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Shahzad" }),
                expected_data: json!({}),
                common: Common { node: Some("db1".into()), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc0_0"), Text::reference("collection0")],
                common: Common { node: Some("db1".into()), capture: captures([("collectionHead3", "/cids/1"), ("head3", "/cids/0")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Chris" }),
                expected_data: json!({}),
                common: Common { node: Some("db1".into()), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc0_0"), Text::reference("collection0")],
                common: Common { node: Some("db1".into()), capture: captures([("collectionHead4", "/cids/1"), ("head4", "/cids/0")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForMerge {
                doc_id: Text::reference("doc0_0"),
                cid: Text::reference("head4"),
                expected_data: json!({}),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForMerge {
                doc_id: Text::reference("collection0"),
                cid: Text::reference("collectionHead4"),
                expected_data: json!({}),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Addo" }),
                expected_data: json!({}),
                common: Common { node: Some("db0".into()), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdates {
                doc_ids: vec![Text::reference("doc0_0"), Text::reference("collection0")],
                common: Common { node: Some("db0".into()), capture: captures([("collectionHead5", "/cids/1"), ("head5", "/cids/0")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForMerge {
                doc_id: Text::reference("doc0_0"),
                cid: Text::reference("head5"),
                expected_data: json!({}),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForMerge {
                doc_id: Text::reference("collection0"),
                cid: Text::reference("collectionHead5"),
                expected_data: json!({}),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits {
                                                cid
                                                links {
                                                    cid
                                                }
                                                heads {
                                                    cid
                                                }
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "cid": m::all(json!([
                                m::same("collectionNode0Update2Cid"),
                                m::unique("uniqueCid-node0"),
                            ])),
                            "heads": [
                                { "cid": m::same("collectionNode0Update1Cid") },
                                { "cid": m::same("collectionNode1Update2Cid") },
                            ],
                            "links": [{ "cid": m::same("docNode0Update2Cid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("collectionNode1Update2Cid"),
                                m::unique("uniqueCid-node0"),
                            ])),
                            "heads": [{ "cid": m::same("collectionCreateCid") }],
                            "links": [{ "cid": m::same("docNode1Update2Cid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("collectionCreateCid"),
                                m::unique("uniqueCid-node0"),
                            ])),
                            "heads": [],
                            "links": [{ "cid": m::same("docCreateCid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("collectionNode0Update1Cid"),
                                m::unique("uniqueCid-node0"),
                            ])),
                            "heads": [{ "cid": m::same("collectionNode1Update1Cid") }],
                            "links": [{ "cid": m::same("docNode0Update1Cid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("collectionNode1Update1Cid"),
                                m::unique("uniqueCid-node0"),
                            ])),
                            "heads": [{ "cid": m::same("collectionCreateCid") }],
                            "links": [{ "cid": m::same("docNode1Update1Cid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("nameNode0Update2Cid"),
                                m::unique("uniqueCid-node0"),
                            ])),
                            "heads": [
                                { "cid": m::same("nameNode1Update2Cid") },
                                { "cid": m::same("nameNode0Update1Cid") },
                            ],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("nameNode0Update1Cid"),
                                m::unique("uniqueCid-node0"),
                            ])),
                            "heads": [{ "cid": m::same("nameNode1Update1Cid") }],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("nameNode1Update1Cid"),
                                m::unique("uniqueCid-node0"),
                            ])),
                            "heads": [{ "cid": m::same("nameCreateCid") }],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("nameCreateCid"), m::unique("uniqueCid-node0")])),
                            "heads": [],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("nameNode1Update2Cid"),
                                m::unique("uniqueCid-node0"),
                            ])),
                            "heads": [{ "cid": m::same("nameCreateCid") }],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("docNode0Update2Cid"),
                                m::unique("uniqueCid-node0"),
                            ])),
                            "heads": [
                                { "cid": m::same("docNode1Update2Cid") },
                                { "cid": m::same("docNode0Update1Cid") },
                            ],
                            "links": [{ "cid": m::same("nameNode0Update2Cid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("docNode0Update1Cid"),
                                m::unique("uniqueCid-node0"),
                            ])),
                            "heads": [{ "cid": m::same("docNode1Update1Cid") }],
                            "links": [{ "cid": m::same("nameNode0Update1Cid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("docNode1Update1Cid"),
                                m::unique("uniqueCid-node0"),
                            ])),
                            "heads": [{ "cid": m::same("docCreateCid") }],
                            "links": [{ "cid": m::same("nameNode1Update1Cid") }],
                        },
                        {
                            "cid": m::all(json!([m::same("docCreateCid"), m::unique("uniqueCid-node0")])),
                            "heads": [],
                            "links": [{ "cid": m::same("nameCreateCid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("docNode1Update2Cid"),
                                m::unique("uniqueCid-node0"),
                            ])),
                            "heads": [{ "cid": m::same("docCreateCid") }],
                            "links": [{ "cid": m::same("nameNode1Update2Cid") }],
                        },
                    ],
                }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits {
                                                cid
                                                links {
                                                    cid
                                                }
                                                heads {
                                                    cid
                                                }
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "cid": m::all(json!([
                                m::same("collectionNode0Update2Cid"),
                                m::unique("uniqueCid-node1"),
                            ])),
                            "heads": [
                                { "cid": m::same("collectionNode0Update1Cid") },
                                { "cid": m::same("collectionNode1Update2Cid") },
                            ],
                            "links": [{ "cid": m::same("docNode0Update2Cid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("collectionNode1Update2Cid"),
                                m::unique("uniqueCid-node1"),
                            ])),
                            "heads": [{ "cid": m::same("collectionCreateCid") }],
                            "links": [{ "cid": m::same("docNode1Update2Cid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("collectionCreateCid"),
                                m::unique("uniqueCid-node1"),
                            ])),
                            "heads": [],
                            "links": [{ "cid": m::same("docCreateCid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("collectionNode0Update1Cid"),
                                m::unique("uniqueCid-node1"),
                            ])),
                            "heads": [{ "cid": m::same("collectionNode1Update1Cid") }],
                            "links": [{ "cid": m::same("docNode0Update1Cid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("collectionNode1Update1Cid"),
                                m::unique("uniqueCid-node1"),
                            ])),
                            "heads": [{ "cid": m::same("collectionCreateCid") }],
                            "links": [{ "cid": m::same("docNode1Update1Cid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("nameNode0Update2Cid"),
                                m::unique("uniqueCid-node1"),
                            ])),
                            "heads": [
                                { "cid": m::same("nameNode1Update2Cid") },
                                { "cid": m::same("nameNode0Update1Cid") },
                            ],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("nameNode0Update1Cid"),
                                m::unique("uniqueCid-node1"),
                            ])),
                            "heads": [{ "cid": m::same("nameNode1Update1Cid") }],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("nameNode1Update1Cid"),
                                m::unique("uniqueCid-node1"),
                            ])),
                            "heads": [{ "cid": m::same("nameCreateCid") }],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([m::same("nameCreateCid"), m::unique("uniqueCid-node1")])),
                            "heads": [],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("nameNode1Update2Cid"),
                                m::unique("uniqueCid-node1"),
                            ])),
                            "heads": [{ "cid": m::same("nameCreateCid") }],
                            "links": [],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("docNode0Update2Cid"),
                                m::unique("uniqueCid-node1"),
                            ])),
                            "heads": [
                                { "cid": m::same("docNode1Update2Cid") },
                                { "cid": m::same("docNode0Update1Cid") },
                            ],
                            "links": [{ "cid": m::same("nameNode0Update2Cid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("docNode0Update1Cid"),
                                m::unique("uniqueCid-node1"),
                            ])),
                            "heads": [{ "cid": m::same("docNode1Update1Cid") }],
                            "links": [{ "cid": m::same("nameNode0Update1Cid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("docNode1Update1Cid"),
                                m::unique("uniqueCid-node1"),
                            ])),
                            "heads": [{ "cid": m::same("docCreateCid") }],
                            "links": [{ "cid": m::same("nameNode1Update1Cid") }],
                        },
                        {
                            "cid": m::all(json!([m::same("docCreateCid"), m::unique("uniqueCid-node1")])),
                            "heads": [],
                            "links": [{ "cid": m::same("nameCreateCid") }],
                        },
                        {
                            "cid": m::all(json!([
                                m::same("docNode1Update2Cid"),
                                m::unique("uniqueCid-node1"),
                            ])),
                            "heads": [{ "cid": m::same("docCreateCid") }],
                            "links": [{ "cid": m::same("nameNode1Update2Cid") }],
                        },
                    ],
                }),
                common: Common { node: Some("db1".into()), ..Default::default() },
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
                "#.into(),
                expected_data: json!({ "Users": [{ "name": "Addo" }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
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
                "#.into(),
                expected_data: json!({ "Users": [{ "name": "Addo" }] }),
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
