//! Generated from the conformance scenarios of tests/integration/acp/nac/disable_nac_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{Close, DisableNAC, GetNACStatus, NewNode, Start};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.nac.lifecycle.disable")]
fn nac_disable_no_identity_when_configured_and_already_disabled_before_error() {
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
            DisableNAC {
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DisableNAC {
                expected_error: Some("node acp is already disabled".into()),
                ..Default::default()
            }
            .into(),
            GetNACStatus {
                expected_data: json!({ "status": "disabled temporarily" }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.nac.lifecycle.disable")]
fn nac_disable_not_configured_before_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![DisableNAC {
            expected_error: Some("node acp is not configured".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.nac.lifecycle.disable")]
fn nac_disable_not_configured_before_with_identity_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![DisableNAC {
            expected_error: Some("node acp is not configured".into()),
            common: Common {
                identity_seed: Some(1),
                ..Default::default()
            },
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.nac.lifecycle.disable")]
fn nac_disable_successfully_then_restart_with_no_args_remains_disabled() {
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
            DisableNAC {
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
                expected_data: json!({ "status": "disabled temporarily" }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetNACStatus {
                expected_data: json!({ "status": "disabled temporarily" }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.nac.lifecycle.disable")]
fn nac_disable_successfully_then_restart_with_start_args_remains_disabled() {
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
            DisableNAC {
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
                expected_data: json!({ "status": "disabled temporarily" }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetNACStatus {
                expected_data: json!({ "status": "disabled temporarily" }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.nac.lifecycle.disable")]
fn nac_disable_with_identity_on_node_that_has_nac_configured_and_enabled_successful() {
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
            GetNACStatus {
                expected_error: Some(
                    "not authorized to perform operation. Permission: get-nac-status".into(),
                ),
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
            GetNACStatus {
                expected_data: json!({ "status": "disabled temporarily" }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetNACStatus {
                expected_data: json!({ "status": "disabled temporarily" }),
                ..Default::default()
            }
            .into(),
            GetNACStatus {
                expected_data: json!({ "status": "disabled temporarily" }),
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
#[mantra_macros::req_test("go.acp.nac.lifecycle.disable")]
fn nac_disable_with_identity_when_configured_and_already_disabled_before_error() {
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
            DisableNAC {
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DisableNAC {
                expected_error: Some("node acp is already disabled".into()),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetNACStatus {
                expected_data: json!({ "status": "disabled temporarily" }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.nac.lifecycle.disable")]
fn nac_disable_with_wrong_identity_on_node_that_has_configured_error() {
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
            DisableNAC {
                expected_error: Some(
                    "not authorized to perform operation. Permission: disable-nac".into(),
                ),
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
#[mantra_macros::req_test("go.acp.nac.lifecycle.disable")]
fn nac_disable_without_identity_on_node_that_has_configured_error() {
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
            DisableNAC {
                expected_error: Some(
                    "not authorized to perform operation. Permission: disable-nac".into(),
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
        ],
        ..Default::default()
    }
    .execute();
}
