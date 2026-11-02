//! Generated from the conformance scenarios of tests/integration/acp/dac/add_policy/with_multi_policies_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::AddDACPolicy;
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.policy.ids")]
fn acp_add_policy_add_duplicate_policy_by_other_creator_valid_policy_i_ds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: a policy
name: test
resources:
- name: users
  permissions:
  - name: delete
  - name: read
  - name: update
"#
                .into(),
                expected_data: json!({
                    "policyID": "1239a04400966b311339f62db50044b1bde70cece2ce9897d69c1bafa5cfab81",
                }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACPolicy {
                policy: r#"
description: a policy
name: test
resources:
- name: users
  permissions:
  - name: delete
  - name: read
  - name: update
"#
                .into(),
                expected_data: json!({
                    "policyID": "166758b3b8f5edd06f46e9079b30b701aadcf3e59e64afe1c46d4242924bd850",
                }),
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
#[mantra_macros::req_test("go.acp.dac.policy.ids")]
fn acp_add_policy_add_multiple_different_policies_valid_policy_i_ds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
                    name: a policy
                    description: a policy
                    resources:
                    - name: users
                      permissions:
                      - name: read
                      - name: update
                      - name: delete
                "#
                .into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACPolicy {
                policy: r#"
                    name: a policy
                    description: another policy
                    resources:
                    - name: users
                      permissions:
                      - name: read
                        expr: reader
                      - name: update
                      - name: delete
                      relations:
                      - name: reader
                        types:
                        - actor
                      - name: admin
                        manages:
                        - reader
                        types:
                        - actor
                "#
                .into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
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
#[mantra_macros::req_test("go.acp.dac.policy.ids")]
fn acp_add_policy_add_multiple_duplicate_policies_different_fmts_produces_different_i_ds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: a policy
name: test
resources:
- name: users
  permissions:
  - name: delete
  - name: read
  - name: update
"#
                .into(),
                expected_data: json!({
                    "policyID": "1239a04400966b311339f62db50044b1bde70cece2ce9897d69c1bafa5cfab81",
                }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACPolicy {
                policy: r#"
description: a policy
name: test
resources:
- name: users
  permissions:
  - name: delete
  - name: read
  - name: update
"#
                .into(),
                expected_data: json!({
                    "policyID": "166758b3b8f5edd06f46e9079b30b701aadcf3e59e64afe1c46d4242924bd850",
                }),
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
#[mantra_macros::req_test("go.acp.dac.policy.ids")]
fn acp_add_policy_add_multiple_duplicate_policies_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: a policy
name: test
resources:
- name: users
  permissions:
  - name: delete
  - name: read
  - name: update
"#
                .into(),
                expected_data: json!({
                    "policyID": "1239a04400966b311339f62db50044b1bde70cece2ce9897d69c1bafa5cfab81",
                }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACPolicy {
                policy: r#"
description: a policy
name: test
resources:
- name: users
  permissions:
  - name: delete
  - name: read
  - name: update
"#
                .into(),
                expected_data: json!({
                    "policyID": "166758b3b8f5edd06f46e9079b30b701aadcf3e59e64afe1c46d4242924bd850",
                }),
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
