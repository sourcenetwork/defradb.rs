//! Generated from the conformance scenarios of tests/integration/acp/dac/index/new_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{IndexField, IndexSpec};
use integration_test::actions::{
    AddCollection, AddDACPolicy, NewIndex, Request, WaitForPeersEvents,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.query.index-maintenance")]
fn acp_index_new_with_directive_on_collection_with_policy_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: a test policy which marks a collection in a database as a resource
name: test
resources:
- name: users
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
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
                    capture: captures([("policy0", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Users @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "users"
					) {
						name: String @index
						age: Int
					}
				"#,
                    ),
                ]),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                        Users {
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.query.index-maintenance")]
fn acp_index_new_with_separate_request_on_collection_with_policy_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: a test policy which marks a collection in a database as a resource
name: test
resources:
- name: users
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
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
                    capture: captures([("policy0", "/policyID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Users @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "users"
					) {
						name: String
						age: Int
					}
				"#,
                    ),
                ]),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "Users".into(),
                index: IndexSpec {
                    name: "some_index".into(),
                    fields: vec![IndexField {
                        name: "name".into(),
                        descending: false,
                    }],
                    unique: false,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                        Users {
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
