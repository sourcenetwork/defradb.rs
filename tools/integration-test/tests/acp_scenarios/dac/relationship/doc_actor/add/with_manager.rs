//! Generated from the conformance scenarios of tests/integration/acp/dac/relationship/doc_actor/add/with_manager_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDACActorRelationship, AddDACPolicy, AddDoc, DeleteDoc, PeerEventsStart,
    Request, UpdateDoc, WaitForUpdate,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.relationship.manager")]
fn acp_cant_make_relationship_if_not_owner_or_manager_error() {
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
            AddDACActorRelationship {
                target_identity_seed: Some(3),
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
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.relationship.manager")]
fn acp_manager_adds_relationship_with_relation_it_does_not_manage_according_to_policy_error() {
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
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(3),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "updater".into(),
                expected_error: Some("UNAUTHORIZED".into()),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(3),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "deleter".into(),
                expected_error: Some("UNAUTHORIZED".into()),
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
                            _docID
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
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Shahzad Lone" }),
                expected_error: Some("document not found or not authorized to access".into()),
                common: Common {
                    identity_seed: Some(3),
                    retry: true,
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
                    identity_seed: Some(3),
                    retry: true,
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
#[mantra_macros::req_test("go.acp.dac.relationship.manager")]
fn acp_manager_gives_read_access_to_another_actor_other_actor_can_read() {
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
"#.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common { identity_seed: Some(1), capture: captures([("policy0", "/policyID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([Text::from(r#"
						type Users @policy(
							id: ""#), Text::reference("policy0"), Text::from(r#"",
							resource: "users"
						) {
							name: String
							age: Int
						}
					"#)]),
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
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "Users": [] }),
                common: Common { identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(3),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "admin".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common { identity_seed: Some(1), ..Default::default() },
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
                doc_id: Text::reference("doc0_0"),
                relation: "reader".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common { identity_seed: Some(3), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "_docID": m::r("doc0_0"), "age": 28, "name": "Shahzad" }] }),
                common: Common { identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Shahzad Lone" }),
                expected_error: Some("document not found or not authorized to access".into()),
                common: Common { identity_seed: Some(2), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                expected_error: Some("document not found or not authorized to access".into()),
                common: Common { identity_seed: Some(2), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.relationship.manager")]
fn acp_manager_gives_write_access_to_another_actor_other_actor_can_write() {
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
    - updater
    - deleter
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
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
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
            AddDACActorRelationship {
                target_identity_seed: Some(3),
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
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(2),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "updater".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(3),
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
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Shahzad Lone" }),
                common: Common {
                    identity_seed: Some(2),
                    retry: true,
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
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "_docID": m::r("doc0_0"), "age": 28, "name": "Shahzad Lone" }],
                }),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
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
                    identity_seed: Some(3),
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
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                common: Common {
                    identity_seed: Some(2),
                    retry: true,
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
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
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
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.relationship.manager")]
fn acp_owner_makes_a_manager_that_gives_it_self_read_access_manager_can_read() {
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
"#.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common { identity_seed: Some(1), capture: captures([("policy0", "/policyID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([Text::from(r#"
						type Users @policy(
							id: ""#), Text::reference("policy0"), Text::from(r#"",
							resource: "users"
						) {
							name: String
							age: Int
						}
					"#)]),
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
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "Users": [] }),
                common: Common { identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(2),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "admin".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common { identity_seed: Some(1), ..Default::default() },
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
                doc_id: Text::reference("doc0_0"),
                relation: "reader".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common { identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "_docID": m::r("doc0_0"), "age": 28, "name": "Shahzad" }] }),
                common: Common { identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Shahzad Lone" }),
                expected_error: Some("document not found or not authorized to access".into()),
                common: Common { identity_seed: Some(2), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                expected_error: Some("document not found or not authorized to access".into()),
                common: Common { identity_seed: Some(2), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.relationship.manager")]
fn acp_owner_makes_a_manager_that_gives_it_self_read_and_write_access_manager_can_read_and_write() {
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
    - updater
    - deleter
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
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
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
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Shahzad Lone" }),
                expected_error: Some("document not found or not authorized to access".into()),
                common: Common {
                    identity_seed: Some(2),
                    retry: true,
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
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(2),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "updater".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(2),
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
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Shahzad Lone" }),
                common: Common {
                    identity_seed: Some(2),
                    retry: true,
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
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "_docID": m::r("doc0_0"), "age": 28, "name": "Shahzad Lone" }],
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
            AddDACActorRelationship {
                target_identity_seed: Some(2),
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "deleter".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(2),
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
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                common: Common {
                    identity_seed: Some(2),
                    retry: true,
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
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
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
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.relationship.manager")]
fn acp_owner_makes_manager_but_manager_can_not_perform_operations_manager_cant_read_or_write() {
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
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
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
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Shahzad Lone" }),
                expected_error: Some("document not found or not authorized to access".into()),
                common: Common {
                    identity_seed: Some(2),
                    retry: true,
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
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
