//! Generated from the conformance scenarios of tests/integration/acp/dac/relationship/doc_actor/delete/with_manager_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDACActorRelationship, AddDACPolicy, AddDoc, DeleteDACActorRelationship,
    Request, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.relationship.revoke-forbidden")]
fn acp_admin_tries_to_revoke_owners_access_not_allowed_error() {
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
            WaitForPeersEvents {
                start: true,
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
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(2),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "admin".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                ..Default::default()
            }
            .into(),
            DeleteDACActorRelationship {
                target_identity_seed: Some(1),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "owner".into(),
                expected_error: Some("OPERATION_FORBIDDEN".into()),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteDACActorRelationship {
                target_identity_seed: Some(2),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "admin".into(),
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

#[test]
#[mantra_macros::req_test("go.acp.dac.relationship.revoke")]
fn acp_manager_revokes_read_access_other_actor_can_no_longer_read() {
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
            WaitForPeersEvents {
                start: true,
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
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(2),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "admin".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(3),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "reader".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
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
                    identity_seed: Some(3),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteDACActorRelationship {
                target_identity_seed: Some(3),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "reader".into(),
                expected_data: json!({ "recordFound": true }),
                common: Common {
                    identity_seed: Some(2),
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
                    identity_seed: Some(3),
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

#[test]
#[mantra_macros::req_test("go.acp.dac.relationship.revoke")]
fn acp_owner_revokes_managers_access_manager_can_no_longer_manage_others() {
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
            WaitForPeersEvents {
                start: true,
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
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(2),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "admin".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(3),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "reader".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
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
                    identity_seed: Some(3),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteDACActorRelationship {
                target_identity_seed: Some(2),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "admin".into(),
                expected_data: json!({ "recordFound": true }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(4),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "reader".into(),
                expected_error: Some("failed to add document actor relationship with acp".into()),
                common: Common {
                    identity_seed: Some(2),
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
                    identity_seed: Some(4),
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
