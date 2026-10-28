//! Generated from the conformance scenarios of tests/integration/acp/nac/relation_admin/refresh_view_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddNACActorRelationship, NodeStopForRestart, RefreshViews, Start};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.nac.admin.schema-tools")]
fn nac_admin_relation_can_refresh_views() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeStopForRestart {
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
                node_identity_seed: Some(0),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            RefreshViews {
                expected_error: Some(
                    "not authorized to perform operation. Permission: refresh-view".into(),
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
            RefreshViews {
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
