//! Generated from the conformance scenarios of tests/integration/acp/nac/nac_start_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{Close, GetNACStatus, Start};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.nac.lifecycle.start")]
fn nac_start_node_no_identity_with_nac_enable_true_error_as_identity_is_needed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            GetNACStatus {
                expected_data: json!({ "status": "not configured" }),
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
                expected_error: Some("can not start nac without identity".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.nac.lifecycle.start")]
fn nac_start_node_with_identity_and_with_nac_enable_false_nac_not_enabled() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            GetNACStatus {
                expected_data: json!({ "status": "not configured" }),
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
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetNACStatus {
                expected_data: json!({ "status": "not configured" }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.nac.lifecycle.start")]
fn nac_start_node_with_identity_and_with_nac_enable_true_nac_enabled_successfully() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            GetNACStatus {
                expected_data: json!({ "status": "not configured" }),
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
                expected_error: Some("not authorized to perform operation".into()),
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
#[mantra_macros::req_test("go.acp.nac.lifecycle.start")]
fn nac_start_with_default_config_nac_status_is_disabled() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![GetNACStatus {
            expected_data: json!({ "status": "not configured" }),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.nac.lifecycle.start")]
fn nac_start_with_default_config_with_identity_nac_status_is_disabled() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![GetNACStatus {
            expected_data: json!({ "status": "not configured" }),
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
