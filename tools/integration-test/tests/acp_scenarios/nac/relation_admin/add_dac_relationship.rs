//! Generated from the conformance scenarios of tests/integration/acp/nac/relation_admin/add_dac_relationship_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDACActorRelationship, AddDACPolicy, AddDoc, AddNACActorRelationship, Close,
    Start, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.nac.admin.dac-relationships")]
fn nac_admin_relation_with_dac_manager_relation_can_add_dac_actor_relationship() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            Close {
                keep_data: true,
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                common: Common {
                    identity_seed: Some(1),
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
  - expr: reader + updater + deleter
    name: read
  - expr: updater
    name: update
  relations:
  - name: deleter
    types:
    - actor
  - manages:
    - reader
    - updater
    - deleter
    name: manager
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
                    Text::from(r#"type Users @policy(id: ""#),
                    Text::reference("policy0"),
                    Text::from(r#"", resource: "users") { name: String }"#),
                ]),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
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
                doc_map: Some(json!({ "name": "Shahzad" })),
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
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "manager".into(),
                target_identity_seed: Some(2),
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
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "reader".into(),
                target_identity_seed: Some(3),
                expected_error: Some(
                    "not authorized to perform operation. Permission: add-dac-relation".into(),
                ),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(2),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "reader".into(),
                target_identity_seed: Some(3),
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
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.nac.admin.dac-relationships")]
fn nac_admin_relation_without_manager_dac_relation_can_not_add_dac_actor_relationship() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            Close {
                keep_data: true,
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                common: Common {
                    identity_seed: Some(1),
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
  - expr: reader + updater + deleter
    name: read
  - expr: updater
    name: update
  relations:
  - name: deleter
    types:
    - actor
  - manages:
    - reader
    - updater
    - deleter
    name: manager
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
                    Text::from(r#"type Users @policy(id: ""#),
                    Text::reference("policy0"),
                    Text::from(r#"", resource: "users") { name: String }"#),
                ]),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
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
                doc_map: Some(json!({ "name": "Shahzad" })),
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
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "reader".into(),
                target_identity_seed: Some(2),
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
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "reader".into(),
                target_identity_seed: Some(3),
                expected_error: Some(
                    "not authorized to perform operation. Permission: add-dac-relation".into(),
                ),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(2),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                relation: "reader".into(),
                target_identity_seed: Some(3),
                expected_error: Some(
                    "cannot create relationship: actor is not a manager of relation".into(),
                ),
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
