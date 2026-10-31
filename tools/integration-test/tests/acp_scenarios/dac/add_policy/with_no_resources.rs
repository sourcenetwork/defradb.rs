//! Generated from the conformance scenarios of tests/integration/acp/dac/add_policy/with_no_resources_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::AddDACPolicy;
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.policy.resources")]
fn acp_add_policy_no_resource_label_valid_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddDACPolicy {
            policy: r#"
                    name: test
                    description: a policy
                "#
            .into(),
            expected_data: json!({ "policyID": m::nonempty_string() }),
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
#[mantra_macros::req_test("go.acp.dac.policy.resources")]
fn acp_add_policy_no_resource_valid_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddDACPolicy {
            policy: r#"
                    name: test
                    description: a policy
                    resources:
                "#
            .into(),
            expected_data: json!({ "policyID": m::nonempty_string() }),
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
#[mantra_macros::req_test("go.acp.dac.policy.missing-input")]
fn acp_add_policy_policy_with_only_space_name_is_required() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddDACPolicy {
            policy: " ".into(),
            expected_error: Some("name required".into()),
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
