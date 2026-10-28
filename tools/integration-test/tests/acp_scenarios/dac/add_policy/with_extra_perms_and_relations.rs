//! Generated from the conformance scenarios of tests/integration/acp/dac/add_policy/with_extra_perms_and_relations_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::AddDACPolicy;
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.policy.permissions")]
fn acp_add_policy_extra_permissions_and_extra_relations_valid_policy_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddDACPolicy {
            policy: r#"
description: a policy
name: test
resources:
- name: users
  permissions:
  - name: delete
  - expr: joker
    name: extra
  - expr: reader
    name: read
  - name: update
  relations:
  - name: joker
    types:
    - actor
  - name: reader
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
        .into()],
        ..Default::default()
    }
    .execute();
}
