//! Generated from the conformance scenarios of tests/integration/acp/dac/add_policy/with_perm_invalid_expr_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::AddDACPolicy;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.acp.dac.policy.invalid")]
fn acp_add_policy_expression_references_undeclared_relation_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddDACPolicy {
            policy: r#"
description: a policy
name: a policy
resources:
- name: users
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
"#
            .into(),
            expected_error: Some("BAD_INPUT".into()),
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
#[mantra_macros::req_test("go.acp.dac.policy.invalid")]
fn acp_add_policy_permission_expr_referencing_owner_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddDACPolicy {
            policy: r#"
name: test
description: a policy
resources:
- name: users
  permissions:
  - expr: reader + owner
    name: read
  relations:
  - name: reader
    types:
    - actor
"#
            .into(),
            expected_error: Some("permission cannot reference `owner`".into()),
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
#[mantra_macros::req_test("go.acp.dac.policy.invalid")]
fn acp_add_policy_permission_expr_with_incorrect_symbol_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddDACPolicy {
            policy: r#"
name: test
description: a policy
resources:
- name: users
  permissions:
  - name: delete
  - expr: reader ^ asf
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
"#
            .into(),
            expected_error: Some("token recognition error".into()),
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
