//! Generated from the conformance scenarios of tests/integration/acp/dac/link_collection/reject_missing_dri_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDACPolicy, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.link.bad-policy")]
fn acp_link_collection_where_a_policy_was_added_but_linked_policy_was_not_added_collection_rejected(
) {
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
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
"#
                .into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common {
                    identity_seed: Some(1),
                    capture: captures([("policy0", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users @policy(
                        id: "1239a04400966b311339f62db50044b1bde70cece2ce9897d69c1bafa5cfab81",
                        resource: "users"
                    ) {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                expected_error: Some("policyID specified does not exist with acp".into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type (name: "Users") {
                            name
                            fields {
                                name
                                type {
                                name
                                kind
                                }
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "__type": null }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.link.bad-policy")]
fn acp_link_collection_where_no_policy_was_added_collection_rejected() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users @policy(
                        id: "1239a04400966b311339f62db50044b1bde70cece2ce9897d69c1bafa5cfab81",
                        resource: "users"
                    ) {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                expected_error: Some("policyID specified does not exist with acp".into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        __type (name: "Users") {
                            name
                            fields {
                                name
                                type {
                                name
                                kind
                                }
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "__type": null }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
