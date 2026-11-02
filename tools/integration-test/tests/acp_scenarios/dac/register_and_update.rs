//! Generated from the conformance scenarios of tests/integration/acp/dac/register_and_update_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDACPolicy, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.ownership.update")]
fn acp_add_with_identity_and_update_with_identity_can_update() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 28 })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Shahzad Lone" }),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "_docID": m::r("doc0_0"), "age": 28, "name": "Shahzad Lone" }],
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
#[mantra_macros::req_test("go.acp.dac.ownership.update")]
fn acp_add_with_identity_and_update_with_wrong_identity_can_not_update() {
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
"#.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common { identity_seed: Some(1), capture: captures([("policy0", "/policyID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([Text::from(r#"
					type Users @policy(
						id: ""#), Text::reference("policy0"), Text::from(r#"",
						resource: "users"
					) {
						name: String
						age: Int
					}
				"#)]),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 28 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Shahzad Lone" }),
                expected_error: Some("document not found or not authorized to access".into()),
                common: Common { identity_seed: Some(2), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "_docID": m::r("doc0_0"), "age": 28, "name": "Shahzad" }] }),
                common: Common { identity_seed: Some(1), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.ownership.update-gql")]
fn acp_add_with_identity_and_update_with_wrong_identity_gql_can_not_update() {
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
"#.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common { identity_seed: Some(1), capture: captures([("policy0", "/policyID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([Text::from(r#"
					type Users @policy(
						id: ""#), Text::reference("policy0"), Text::from(r#"",
						resource: "users"
					) {
						name: String
						age: Int
					}
				"#)]),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_Users(input: {age: 28, name: "Shahzad"}) { _docID } }"#.into(),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc0_0", "/data/add_Users/0/_docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"mutation { update_Users(docID: ""#), Text::reference("doc0_0"), Text::from(r#"", input: {name: "Shahzad Lone"}) { _docID } }"#)]),
                common: Common { identity_seed: Some(2), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "_docID": m::r("doc0_0"), "age": 28, "name": "Shahzad" }] }),
                common: Common { identity_seed: Some(1), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.ownership.update")]
fn acp_add_with_identity_and_update_without_identity_can_not_update() {
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
"#.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common { identity_seed: Some(1), capture: captures([("policy0", "/policyID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([Text::from(r#"
					type Users @policy(
						id: ""#), Text::reference("policy0"), Text::from(r#"",
						resource: "users"
					) {
						name: String
						age: Int
					}
				"#)]),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 28 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Shahzad Lone" }),
                expected_error: Some("document not found or not authorized to access".into()),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "_docID": m::r("doc0_0"), "age": 28, "name": "Shahzad" }] }),
                common: Common { identity_seed: Some(1), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.ownership.update-gql")]
fn acp_add_with_identity_and_update_without_identity_gql_can_not_update() {
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
"#.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common { identity_seed: Some(1), capture: captures([("policy0", "/policyID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([Text::from(r#"
					type Users @policy(
						id: ""#), Text::reference("policy0"), Text::from(r#"",
						resource: "users"
					) {
						name: String
						age: Int
					}
				"#)]),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_Users(input: {age: 28, name: "Shahzad"}) { _docID } }"#.into(),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc0_0", "/data/add_Users/0/_docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"mutation { update_Users(docID: ""#), Text::reference("doc0_0"), Text::from(r#"", input: {name: "Shahzad Lone"}) { _docID } }"#)]),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "_docID": m::r("doc0_0"), "age": 28, "name": "Shahzad" }] }),
                common: Common { identity_seed: Some(1), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.ownership.update")]
fn acp_add_without_identity_and_update_with_identity_can_update() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 28 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Shahzad Lone" }),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "_docID": m::r("doc0_0"), "age": 28, "name": "Shahzad Lone" }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.ownership.update")]
fn acp_add_without_identity_and_update_without_identity_can_update() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 28 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Shahzad Lone" }),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            _docID
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "_docID": m::r("doc0_0"), "age": 28, "name": "Shahzad Lone" }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
