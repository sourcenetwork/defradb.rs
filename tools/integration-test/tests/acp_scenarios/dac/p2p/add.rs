//! Generated from the conformance scenarios of tests/integration/acp/dac/p2p/add_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddCollectionSubscription, AddDACActorRelationship, AddDACPolicy, AddDoc,
    ConnectPeers, NewNode, Request, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.p2p.remote-dac")]
fn acp_p2_p_add_private_document_and_sync_after_adding_relationship_remote_dac() {
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
                with_identity: true,
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
                with_identity: true,
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
            AddDACPolicy {
                policy: r#"
description: A Policy
name: Test Policy
resources:
- name: users
  permissions:
  - expr: deleter
    name: delete
  - expr: dummy
    name: nothing
  - expr: reader + updater + deleter
    name: read
  - expr: updater
    name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: deleter
    types:
    - actor
  - name: dummy
    types:
    - actor
  - name: reader
    types:
    - actor
  - name: updater
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
                doc_map: Some(json!({ "name": "Shahzad" })),
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
            Request {
                request: r#"
                    query {
                                        Users{
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(2),
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
                    capture: captures([("head1", "/cid")]),
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
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Shahzad" }] }),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(2),
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
#[mantra_macros::req_test("go.acp.dac.p2p.remote-dac")]
fn acp_p2_p_add_private_documents_on_different_nodes_remote_dac() {
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
                with_identity: true,
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
                with_identity: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACPolicy {
                policy: r#"
description: A Policy
name: Test Policy
resources:
- name: users
  permissions:
  - expr: deleter
    name: delete
  - expr: dummy
    name: nothing
  - expr: reader + updater + deleter
    name: read
  - expr: updater
    name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: deleter
    types:
    - actor
  - name: dummy
    types:
    - actor
  - name: reader
    types:
    - actor
  - name: updater
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
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
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad Lone" })),
                common: Common {
                    node: Some("db1".into()),
                    identity_seed: Some(2),
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
