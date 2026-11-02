//! Generated from the conformance scenarios of tests/integration/encryption/peer_acp_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddCollectionSubscription, AddDACActorRelationship, AddDACPolicy, AddDoc, Close,
    ConnectPeers, NewNode, Request, Wait, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.encryption.peer.acp")]
fn doc_encryption_acp_if_client_node_has_doc_permission_but_server_node_is_not_available_should_not_fetch(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                is_document_acp_test: true,
                common: Common {
                    node: Some("service".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                node_identity_seed: Some(0),
                network: true,
                kms: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                node_identity_seed: Some(1),
                network: true,
                kms: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                node_identity_seed: Some(2),
                network: true,
                kms: true,
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACPolicy {
                policy: r#"
name: Test Policy

description: A Policy

resources:
  - name: users
    permissions:
    - name: read
      expr: reader + updater + deleter
    - name: update
      expr: updater
    - name: delete
      expr: deleter
    - name: nothing
      expr: dummy

    relations:
    - name: reader
      types:
      - actor
    - name: updater
      types:
      - actor
    - name: deleter
      types:
      - actor
    - name: admin
      manages:
      - reader
      types:
      - actor
    - name: dummy
      types:
      - actor
"#
                .into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    node: Some("db0".into()),
                    identity_node: Some("db0".into()),
                    capture: captures([("policy", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Users @policy(
						id: ""#,
                    ),
                    Text::reference("policy"),
                    Text::from(
                        r#"",
						resource: "users"
					) {
						name: String
						age: Int
					}
				"#,
                    ),
                ]),
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
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Users @policy(
						id: ""#,
                    ),
                    Text::reference("policy"),
                    Text::from(
                        r#"",
						resource: "users"
					) {
						name: String
						age: Int
					}
				"#,
                    ),
                ]),
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
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Users @policy(
						id: ""#,
                    ),
                    Text::reference("policy"),
                    Text::from(
                        r#"",
						resource: "users"
					) {
						name: String
						age: Int
					}
				"#,
                    ),
                ]),
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
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 33 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    identity_node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Close {
                keep_data: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                collection: "Users".into(),
                doc_id: Text::reference("doc0"),
                relation: "reader".into(),
                target_identity_node: Some("db1".into()),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    node: Some("db1".into()),
                    identity_node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Wait {
                millis: Some(100),
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
                expected_data: json!({ "Users": [] }),
                common: Common {
                    node: Some("db1".into()),
                    identity_node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Close {
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Close {
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
#[mantra_macros::req_test("go.encryption.peer.acp")]
fn doc_encryption_acp_if_node_has_access_to_some_docs_should_fetch_only_them() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                is_document_acp_test: true,
                common: Common {
                    node: Some("service".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                node_identity_seed: Some(0),
                network: true,
                kms: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                node_identity_seed: Some(1),
                network: true,
                kms: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACPolicy {
                policy: r#"
name: Test Policy

description: A Policy

resources:
  - name: users
    permissions:
    - name: read
      expr: reader + updater + deleter
    - name: update
      expr: updater
    - name: delete
      expr: deleter
    - name: nothing
      expr: dummy

    relations:
    - name: reader
      types:
      - actor
    - name: updater
      types:
      - actor
    - name: deleter
      types:
      - actor
    - name: admin
      manages:
      - reader
      types:
      - actor
    - name: dummy
      types:
      - actor
"#
                .into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    node: Some("db0".into()),
                    identity_node: Some("db0".into()),
                    capture: captures([("policy", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Users @policy(
						id: ""#,
                    ),
                    Text::reference("policy"),
                    Text::from(
                        r#"",
						resource: "users"
					) {
						name: String
						age: Int
					}
				"#,
                    ),
                ]),
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
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Users @policy(
						id: ""#,
                    ),
                    Text::reference("policy"),
                    Text::from(
                        r#"",
						resource: "users"
					) {
						name: String
						age: Int
					}
				"#,
                    ),
                ]),
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
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 33 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    identity_node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                collection: "Users".into(),
                doc_id: Text::reference("doc0"),
                relation: "reader".into(),
                target_identity_node: Some("db1".into()),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    node: Some("db0".into()),
                    identity_node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head1", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 33 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    identity_node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc1")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head2", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 33 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc2")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head3", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 33 })),
                is_doc_encrypted: Some(false),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    identity_node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc3")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head4", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                collection: "Users".into(),
                doc_id: Text::reference("doc3"),
                relation: "reader".into(),
                target_identity_node: Some("db1".into()),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    node: Some("db0".into()),
                    identity_node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc3")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head5", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Keenan", "age": 33 })),
                is_doc_encrypted: Some(false),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    identity_node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc4")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head6", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 33 })),
                is_doc_encrypted: Some(false),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc5")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head7", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                cid: Some(Text::reference("head1")),
                expected_data: json!({}),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc2")),
                cid: Some(Text::reference("head3")),
                expected_data: json!({}),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc3")),
                cid: Some(Text::reference("head5")),
                expected_data: json!({}),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc5")),
                cid: Some(Text::reference("head7")),
                expected_data: json!({}),
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
                expected_data: json!({
                    "Users": m::unordered(json!([
                        { "name": "Fred" },
                        { "name": "John" },
                        { "name": "Islam" },
                        { "name": "Shahzad" },
                    ])),
                }),
                common: Common {
                    node: Some("db1".into()),
                    identity_node: Some("db1".into()),
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
#[mantra_macros::req_test("go.encryption.peer.acp")]
fn doc_encryption_acp_if_user_and_node_have_access_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                is_document_acp_test: true,
                common: Common {
                    node: Some("service".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                node_identity_seed: Some(0),
                network: true,
                kms: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                node_identity_seed: Some(1),
                network: true,
                kms: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACPolicy {
                policy: r#"
name: Test Policy

description: A Policy

resources:
  - name: users
    permissions:
    - name: read
      expr: reader + updater + deleter
    - name: update
      expr: updater
    - name: delete
      expr: deleter
    - name: nothing
      expr: dummy

    relations:
    - name: reader
      types:
      - actor
    - name: updater
      types:
      - actor
    - name: deleter
      types:
      - actor
    - name: admin
      manages:
      - reader
      types:
      - actor
    - name: dummy
      types:
      - actor
"#
                .into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    node: Some("db0".into()),
                    identity_seed: Some(2),
                    capture: captures([("policy", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Users @policy(
						id: ""#,
                    ),
                    Text::reference("policy"),
                    Text::from(
                        r#"",
						resource: "users"
					) {
						name: String
						age: Int
					}
				"#,
                    ),
                ]),
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
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Users @policy(
						id: ""#,
                    ),
                    Text::reference("policy"),
                    Text::from(
                        r#"",
						resource: "users"
					) {
						name: String
						age: Int
					}
				"#,
                    ),
                ]),
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
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 33 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    identity_seed: Some(2),
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                collection: "Users".into(),
                doc_id: Text::reference("doc0"),
                relation: "reader".into(),
                target_identity_seed: Some(3),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    node: Some("db0".into()),
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head1", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                collection: "Users".into(),
                doc_id: Text::reference("doc0"),
                relation: "reader".into(),
                target_identity_node: Some("db1".into()),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    node: Some("db0".into()),
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head2", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                cid: Some(Text::reference("head2")),
                expected_data: json!({}),
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
                expected_data: json!({ "Users": [{ "name": "Fred" }] }),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(3),
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
#[mantra_macros::req_test("go.encryption.peer.acp")]
fn doc_encryption_acp_if_user_has_access_but_not_node_should_not_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                is_document_acp_test: true,
                common: Common {
                    node: Some("service".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                node_identity_seed: Some(0),
                network: true,
                kms: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                node_identity_seed: Some(1),
                network: true,
                kms: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACPolicy {
                policy: r#"
name: Test Policy

description: A Policy

resources:
  - name: users
    permissions:
    - name: read
      expr: reader + updater + deleter
    - name: update
      expr: updater
    - name: delete
      expr: deleter
    - name: nothing
      expr: dummy

    relations:
    - name: reader
      types:
      - actor
    - name: updater
      types:
      - actor
    - name: deleter
      types:
      - actor
    - name: admin
      manages:
      - reader
      types:
      - actor
    - name: dummy
      types:
      - actor
"#
                .into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    node: Some("db0".into()),
                    identity_seed: Some(2),
                    capture: captures([("policy", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Users @policy(
						id: ""#,
                    ),
                    Text::reference("policy"),
                    Text::from(
                        r#"",
						resource: "users"
					) {
						name: String
						age: Int
					}
				"#,
                    ),
                ]),
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
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Users @policy(
						id: ""#,
                    ),
                    Text::reference("policy"),
                    Text::from(
                        r#"",
						resource: "users"
					) {
						name: String
						age: Int
					}
				"#,
                    ),
                ]),
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
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 33 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    identity_seed: Some(2),
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                collection: "Users".into(),
                doc_id: Text::reference("doc0"),
                relation: "reader".into(),
                target_identity_seed: Some(3),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    node: Some("db0".into()),
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head1", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Wait {
                millis: Some(100),
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
                expected_data: json!({ "Users": [] }),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(3),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            delta
                            docID
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "_commits": [] }),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(3),
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
