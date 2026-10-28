//! Generated from the conformance scenarios of tests/integration/searchable_encryption/replicator_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, AddReplicator, NodeOpenP2PSEPersistent, NodeStop, PeerEventsStart,
    Request, Restart, WaitForSESync, WaitForUpdate,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.searchable_encryption.replication")]
fn se_replicator_if_doc_added_while_replicator_is_offline_should_retry() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2PSEPersistent {
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NodeOpenP2PSEPersistent {
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
                sdl: r#"
                    type Users {
                        name: String @encryptedIndex
                        age: Int
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String @encryptedIndex
                        age: Int
                    }
                "#.into(),
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
            NodeStop {
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0"),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 22 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1"),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Restart {
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSESync {
                doc_ids: vec![Text::reference("doc0"), Text::reference("doc1")],
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        encrypted_Users(filter: {name: {_eq: "John"}}) {
                                            docIDs
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "encrypted_Users": [{ "docIDs": m::unordered(json!([m::r("doc0")])) }] }),
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
