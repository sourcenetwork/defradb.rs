//! Generated from the conformance scenarios of tests/integration/acp/dac/commits/with_cid_shared_field_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDACPolicy, AddDoc, PeerEventsStart, Request, WaitForUpdateFields,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.query.shared-cid")]
fn acp_query_commits_with_shared_field_cid_returns_only_accessible_owner() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: a test policy
name: test
resources:
- name: users
  permissions:
  - name: delete
  - name: read
    expr: reader
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
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shared", "age": 28 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdateFields {
                doc_id: Text::reference("doc0_0"),
                common: Common { capture: captures([("cid0_0_0", "/cid"), ("field0_0_name_0", "/fieldCIDs/name")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shared", "age": 29 })),
                common: Common { identity_seed: Some(2), retry: true, capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdateFields {
                doc_id: Text::reference("doc0_1"),
                common: Common { capture: captures([("cid0_1_0", "/cid"), ("field0_1_name_0", "/fieldCIDs/name")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query {
						_commits(cid: ""#), Text::reference("field0_0_name_0"), Text::from(r#"") {
							fieldName
							docID
						}
					}"#)]),
                expected_data: json!({ "_commits": [{ "docID": m::r("doc0_0"), "fieldName": "name" }] }),
                common: Common { identity_seed: Some(1), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query {
						_commits(cid: ""#), Text::reference("field0_1_name_0"), Text::from(r#"") {
							fieldName
							docID
						}
					}"#)]),
                expected_data: json!({ "_commits": [{ "docID": m::r("doc0_1"), "fieldName": "name" }] }),
                common: Common { identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query {
						_commits(cid: ""#), Text::reference("field0_0_name_0"), Text::from(r#"") {
							fieldName
							docID
						}
					}"#)]),
                expected_data: json!({ "_commits": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
