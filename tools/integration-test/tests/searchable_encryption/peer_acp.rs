//! Generated from the conformance scenarios of tests/integration/searchable_encryption/peer_acp_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDACPolicy, AddDoc, AddReplicator, NewNode, Request, WaitForPeersEvents,
    WaitForSESync, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.searchable_encryption.replication")]
fn doc_encryption_peer_with_acp_replicator_should_not_have_access() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                enable_searchable_encryption: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                enable_searchable_encryption: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
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
    doc: |
      placeholder permission, to show a policy can contain any user defined relation,
      in addition to the defra required ones
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
"#.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common { node: Some("db0".into()), identity_node: Some("db0".into()), capture: captures([("policy0", "/policyID")]), ..Default::default() },
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
    doc: |
      placeholder permission, to show a policy can contain any user defined relation,
      in addition to the defra required ones
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
"#.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common { node: Some("db1".into()), identity_node: Some("db0".into()), capture: captures([("policy1", "/policyID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([Text::from(r#"
					type User @policy(
						id: ""#), Text::reference("policy0"), Text::from(r#"",
						resource: "users"
					) {
						name: String
						age: Int @encryptedIndex
					}
				"#)]),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([Text::from(r#"
					type User @policy(
						id: ""#), Text::reference("policy1"), Text::from(r#"",
						resource: "users"
					) {
						name: String
						age: Int @encryptedIndex
					}
				"#)]),
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
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common { node: Some("db0".into()), identity_node: Some("db0".into()), retry: true, capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common { node: Some("db0".into()), ..Default::default() },
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
                        encrypted_User(filter: {age: {_eq: 21}}) {
                            docIDs
                        }
                    }
                "#.into(),
                expected_data: json!({ "encrypted_User": [{ "docIDs": m::unordered(json!([m::r("doc0")])) }] }),
                common: Common { node: Some("db0".into()), identity_node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User {
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "User": [] }),
                common: Common { node: Some("db1".into()), identity_node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            delta
                        }
                    }
                "#.into(),
                expected_data: json!({ "_commits": [] }),
                common: Common { node: Some("db1".into()), identity_node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
