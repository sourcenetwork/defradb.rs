//! Generated from the conformance scenarios of tests/integration/collection_version/updates/replace/view_policy_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDACPolicy, AddView, PatchCollection};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.dematerialize")]
fn col_version_update_replace_is_materialized_given_policy_on_non_m_aterialized_view_errors() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
                    name: test
                    description: a test policy which marks a collection in a database as a resource
                    resources:
                    - name: userView
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
                    capture: captures([("policy", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddView {
                sdl: r#"
                    type UserView @policy(
                        id: "62cff38630eb2732c5f5e763ab31478a4bac7077ed66c9ad0c061c86a5b498c9",
                        resource: "userView"
                    ) @materialized(if: false) {
                        name: String
                    }
                "#
                .into(),
                query: r#"
                    User {
                        name
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "replace",
							"path": "/UserView/IsMaterialized",
							"value": true
						}
					]
				"#
                .into(),
                expected_error: Some(
                    "materialized views do not support ACP. Collection: UserView".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
