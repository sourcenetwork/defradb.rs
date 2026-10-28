//! Generated from the conformance scenarios of tests/integration/acp/nac/delete_nac_relationship_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddNACActorRelationship, DeleteNACActorRelationship, DisableNAC, GetNACStatus,
    NodeStopForRestart, Start,
};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.nac.relationship.delete")]
fn nac_delete_relationship_for_all_identities_all_implicit_identities_access_revoked() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeStopForRestart {
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
            AddNACActorRelationship {
                relation: "admin".into(),
                target_actor: Some("*".into()),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(3),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_actor: Some("*".into()),
                expected_data: json!({ "recordFound": true }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetNACStatus {
                expected_error: Some(
                    "not authorized to perform operation. Permission: get-nac-status".into(),
                ),
                common: Common {
                    identity_seed: Some(4),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetNACStatus {
                expected_error: Some(
                    "not authorized to perform operation. Permission: get-nac-status".into(),
                ),
                ..Default::default()
            }
            .into(),
            GetNACStatus {
                expected_data: json!({ "status": "enabled" }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetNACStatus {
                expected_data: json!({ "status": "enabled" }),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetNACStatus {
                expected_data: json!({ "status": "enabled" }),
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
#[mantra_macros::req_test("go.acp.nac.relationship.delete")]
fn nac_delete_relationship_still_requires_identity_even_if_all_identities_given_access_still_need_identity(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeStopForRestart {
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
            AddNACActorRelationship {
                relation: "admin".into(),
                target_actor: Some("*".into()),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(3),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(3),
                expected_error: Some("node acp relationship operation requires identity".into()),
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(3),
                expected_data: json!({ "recordFound": true }),
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
#[mantra_macros::req_test("go.acp.nac.relationship.delete")]
fn nac_delete_relationship_when_nac_is_disabled_with_invalid_identities_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeStopForRestart {
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
            DisableNAC {
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(3),
                expected_error: Some("operation requires ACP, but ACP not available".into()),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(3),
                expected_error: Some("operation requires ACP, but ACP not available".into()),
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_actor: Some("".into()),
                expected_error: Some("operation requires ACP, but ACP not available".into()),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_actor: Some("".into()),
                expected_error: Some("operation requires ACP, but ACP not available".into()),
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_actor: Some("".into()),
                expected_error: Some("operation requires ACP, but ACP not available".into()),
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
#[mantra_macros::req_test("go.acp.nac.relationship.delete")]
fn nac_delete_relationship_when_nac_is_enabled_with_invalid_identities_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeStopForRestart {
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
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(3),
                expected_error: Some(
                    "not authorized to perform operation. Permission: delete-nac-relation".into(),
                ),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(3),
                expected_error: Some(
                    "not authorized to perform operation. Permission: delete-nac-relation".into(),
                ),
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_actor: Some("".into()),
                expected_error: Some(
                    "not authorized to perform operation. Permission: delete-nac-relation".into(),
                ),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_actor: Some("".into()),
                expected_error: Some(
                    "not authorized to perform operation. Permission: delete-nac-relation".into(),
                ),
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_actor: Some("".into()),
                expected_data: json!({ "recordFound": false }),
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
#[mantra_macros::req_test("go.acp.nac.relationship.delete")]
fn nac_delete_relationship_when_nac_not_configured_before_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(2),
                expected_error: Some("node acp is not configured".into()),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(2),
                expected_error: Some("node acp is not configured".into()),
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_actor: Some("".into()),
                expected_error: Some("node acp is not configured".into()),
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
#[mantra_macros::req_test("go.acp.nac.relationship.delete")]
fn nac_delete_relationship_with_invalid_relation_name_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeStopForRestart {
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
            DeleteNACActorRelationship {
                relation: "unknown".into(),
                target_identity_seed: Some(2),
                expected_error: Some("relation not in resource".into()),
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
#[mantra_macros::req_test("go.acp.nac.relationship.delete")]
fn nac_delete_relationship_with_valid_identity_relationship_deleted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeStopForRestart {
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
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(2),
                expected_data: json!({ "recordFound": true }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(2),
                expected_data: json!({ "recordFound": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteNACActorRelationship {
                relation: "admin".into(),
                target_identity_seed: Some(3),
                expected_error: Some(
                    "not authorized to perform operation. Permission: delete-nac-relation".into(),
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
