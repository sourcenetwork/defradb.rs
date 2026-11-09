//! Generated from the conformance scenarios of tests/integration/acp/dac/link_collection/accept_mixed_resources_on_partial_dri_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDACPolicy, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.link.accepted")]
fn acp_link_collection_use_valid_resource_accept_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
name: test
description: A Partially DRI Compliant Policy
resources:
- name: usersInvalid
  permissions:
  - expr: reader
    name: delete
  - expr: reader
    name: update
  relations:
  - name: reader
    types:
    - actor
- name: usersValid
  permissions:
  - name: delete
  - name: read
    expr: reader
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
                sdl: Text::concat([
                    Text::from(
                        r#"
					type Users @policy(
						id: ""#,
                    ),
                    Text::reference("policy0"),
                    Text::from(
                        r#"",
						resource: "usersValid"
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
                                ofType { name kind }
                                }
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "__type": {
                        "fields": [
                            {
                                "name": "AVG",
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": null,
                                    "ofType": { "kind": "SCALAR", "name": "Float" },
                                },
                            },
                            {
                                "name": "COUNT",
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": null,
                                    "ofType": { "kind": "SCALAR", "name": "Int" },
                                },
                            },
                            {
                                "name": "GROUP",
                                "type": {
                                    "kind": "LIST",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "Users" },
                                },
                            },
                            {
                                "name": "MAX",
                                "type": { "kind": "SCALAR", "name": "Float", "ofType": null },
                            },
                            {
                                "name": "MIN",
                                "type": { "kind": "SCALAR", "name": "Float", "ofType": null },
                            },
                            {
                                "name": "SIMILARITY",
                                "type": { "kind": "SCALAR", "name": "Float", "ofType": null },
                            },
                            {
                                "name": "SUM",
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": null,
                                    "ofType": { "kind": "SCALAR", "name": "Float" },
                                },
                            },
                            {
                                "name": "_deleted",
                                "type": { "kind": "SCALAR", "name": "Boolean", "ofType": null },
                            },
                            {
                                "name": "_docID",
                                "type": { "kind": "SCALAR", "name": "ID", "ofType": null },
                            },
                            {
                                "name": "_version",
                                "type": {
                                    "kind": "LIST",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "Commit" },
                                },
                            },
                            {
                                "name": "age",
                                "type": { "kind": "SCALAR", "name": "Int", "ofType": null },
                            },
                            {
                                "name": "name",
                                "type": { "kind": "SCALAR", "name": "String", "ofType": null },
                            },
                        ],
                        "name": "Users",
                    },
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
