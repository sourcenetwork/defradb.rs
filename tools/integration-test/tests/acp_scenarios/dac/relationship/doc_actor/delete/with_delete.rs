//! Generated from the conformance scenarios of tests/integration/acp/dac/relationship/doc_actor/delete/with_delete_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDACActorRelationship, AddDACPolicy, AddDoc, DeleteDACActorRelationship,
    DeleteDoc, PeerEventsStart, Request, WaitForUpdate,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.relationship.revoke")]
fn acp_owner_revokes_delete_access_other_actor_can_no_longer_delete() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
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
                    identity_seed: Some(1),
                    capture: captures([("policy0", "/policyID")]),
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
                    Text::reference("policy0"),
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
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 28 })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad Lone", "age": 28 })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_1"),
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(2),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "deleter".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(2),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_1"),
                relation: "deleter".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_1"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                        query {
                            Users {
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": m::unordered(json!([
                        { "age": 28, "name": "Shahzad Lone" },
                        { "age": 28, "name": "Shahzad" },
                    ])),
                }),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_1"),
                common: Common {
                    identity_seed: Some(2),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_1"),
                ..Default::default()
            }
            .into(),
            DeleteDACActorRelationship {
                target_identity_seed: Some(2),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "deleter".into(),
                expected_data: json!({ "recordFound": true }),
                common: Common {
                    identity_seed: Some(1),
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
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                expected_error: Some("document not found or not authorized to access".into()),
                common: Common {
                    identity_seed: Some(2),
                    retry: true,
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
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "age": 28, "name": "Shahzad" }] }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
