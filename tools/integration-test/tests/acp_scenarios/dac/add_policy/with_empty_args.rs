//! Generated from the conformance scenarios of tests/integration/acp/dac/add_policy/with_empty_args_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::AddDACPolicy;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.acp.dac.policy.missing-input")]
fn acp_add_policy_empty_creator_and_policy_args_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddDACPolicy {
            policy: "".into(),
            expected_error: Some("policy creator can not be empty".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.policy.missing-input")]
fn acp_add_policy_empty_policy_creator_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddDACPolicy {
            policy: r#"
description: a basic policy that satisfies minimum DRI requirements
name: test
resources:
- name: users
  permissions:
  - name: delete
  - name: read
  - name: update
"#
            .into(),
            expected_error: Some("policy creator can not be empty".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.policy.missing-input")]
fn acp_add_policy_empty_policy_data_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddDACPolicy {
            policy: "".into(),
            expected_error: Some("policy data can not be empty".into()),
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
