//! Generated from the conformance scenarios of tests/integration/searchable_encryption/truncate_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::Filter;
use integration_test::actions::{
    AddCollection, AddDoc, AddReplicator, NodeOpenP2PSE, PeerEventsStart, Request, Truncate,
    WaitForMerge, WaitForSESync, WaitForUpdate,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.searchable_encryption.replication")]
fn truncate_removes_searchable_encryption_artifacts() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2PSE {
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NodeOpenP2PSE {
                common: Common { node: Some("db1".into()), ..Default::default() },
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
            AddCollection {
                sdl: "type User { name: String @encryptedIndex }".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type User { name: String @encryptedIndex }".into(),
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
                collection: "User".into(),
                doc_map: Some(json!({ "name": "alice" })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0"),
                common: Common { node: Some("db0".into()), capture: captures([("head0", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForMerge {
                doc_id: Text::reference("doc0"),
                cid: Text::reference("head0"),
                expected_data: json!({}),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSESync {
                doc_ids: vec![Text::reference("doc0")],
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        encrypted_User(filter: {name: {_eq: "alice"}}) { docIDs }
                                    }
                "#.into(),
                expected_data: json!({ "encrypted_User": [{ "docIDs": m::unordered(json!([m::r("doc0")])) }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "User".into(),
                filter: Some(Filter {
                    kind: "map".into(),
                    value: Some(json!({ "_docID": { "_in": [m::r("doc0")] } })),
                }),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        encrypted_User(filter: {name: {_eq: "alice"}}) { docIDs }
                                    }
                "#.into(),
                expected_data: json!({ "encrypted_User": [{ "docIDs": m::length(0) }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
