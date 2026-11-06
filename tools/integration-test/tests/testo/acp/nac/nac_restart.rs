//! Generated from the conformance scenarios of tests/integration/acp/nac/nac_restart_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{Close, GetNACStatus, NewNode, Start};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.nac.lifecycle.restart")]
fn nac_restart_node_with_nac_enabled_with_another_identity_ignore_new_identity_and_restart_with_existing_nac_state(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                database_dir: Some("".into()),
                ..Default::default()
            }
            .into(),
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
            Close {
                keep_data: true,
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: true,
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
#[mantra_macros::req_test("go.acp.nac.lifecycle.restart")]
fn nac_restart_node_with_nac_enabled_with_explicitly_specifying_same_args_restarts_and_nac_is_still_enabled(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                database_dir: Some("".into()),
                ..Default::default()
            }
            .into(),
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
            GetNACStatus {
                expected_data: json!({ "status": "enabled" }),
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
#[mantra_macros::req_test("go.acp.nac.lifecycle.restart")]
fn nac_restart_node_with_nac_enabled_without_nac_args_restarts_and_nac_is_still_enabled() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                database_dir: Some("".into()),
                ..Default::default()
            }
            .into(),
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
            Close {
                keep_data: true,
                ..Default::default()
            }
            .into(),
            Start {
                enable_nac: false,
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
        ],
        ..Default::default()
    }
    .execute();
}
