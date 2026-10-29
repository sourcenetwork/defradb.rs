//! Generated from the conformance scenarios of tests/integration/acp/dac/index/query_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDACPolicy, AddDoc, Request, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.query.indexed")]
fn acp_with_index_upon_querying_private_doc_with_identity_should_fetch() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Islam" })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_1")),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                        Users {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Shahzad" }, { "name": "Islam" }] }),
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
#[mantra_macros::req_test("go.acp.dac.query.indexed")]
fn acp_with_index_upon_querying_private_doc_with_wrong_identity_should_not_fetch() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Islam" })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_1")),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                        Users {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Shahzad" }] }),
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
#[mantra_macros::req_test("go.acp.dac.query.indexed")]
fn acp_with_index_upon_querying_private_doc_without_identity_should_not_fetch() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Islam" })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_1")),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                        Users {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Shahzad" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
