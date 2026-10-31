//! Generated from the conformance scenarios of tests/integration/query/commits/with_cid_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDACPolicy, AddDoc, Request, UpdateDoc, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.cid.lookup")]
fn query_commits_list_of_one() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Users(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"") { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_0", "/data/Users/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
						_commits(
							cid: [""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(
                        r#""]
						) {
							cid
						}
					}"#,
                    ),
                ]),
                expected_data: json!({ "_commits": [{ "cid": m::r("CID0_0_0") }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.cid.lookup")]
fn query_commits_multiple_cids_different_docs() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        verified: Boolean
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query { Users(docID: ""#), Text::reference("doc0_0"), Text::from(r#"") { _version { cid } } }"#)]),
                common: Common { capture: captures([("CID0_0_0", "/data/Users/0/_version/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "Fred" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query { Users(docID: ""#), Text::reference("doc0_1"), Text::from(r#"") { _version { cid } } }"#)]),
                common: Common { capture: captures([("CID0_1_0", "/data/Users/0/_version/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query {
						_commits(
							cid: [""#), Text::reference("CID0_0_0"), Text::from(r#"", ""#), Text::reference("CID0_1_0"), Text::from(r#""]
						) {
							cid
						}
					}"#)]),
                expected_data: json!({ "_commits": [{ "cid": m::r("CID0_0_0") }, { "cid": m::r("CID0_1_0") }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.cid.lookup")]
fn query_commits_multiple_cids_same_doc() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        verified: Boolean
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query { Users(docID: ""#), Text::reference("doc0_0"), Text::from(r#"") { _version { cid } } }"#)]),
                common: Common { capture: captures([("CID0_0_0", "/data/Users/0/_version/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "age": 22 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query { Users(docID: ""#), Text::reference("doc0_0"), Text::from(r#"") { _version { cid } } }"#)]),
                common: Common { capture: captures([("CID0_0_1", "/data/Users/0/_version/0/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([Text::from(r#"query {
						_commits(
							cid: [""#), Text::reference("CID0_0_0"), Text::from(r#"", ""#), Text::reference("CID0_0_1"), Text::from(r#""]
						) {
							cid
						}
					}"#)]),
                expected_data: json!({ "_commits": [{ "cid": m::r("CID0_0_0") }, { "cid": m::r("CID0_0_1") }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.cid.lookup")]
fn query_commits_multiple_cids_different_docs_no_access_to_second_cid() {
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
                    capture: captures([("policy", "/policyID")]),
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
                    Text::reference("policy"),
                    Text::from(
                        r#"",
						resource: "users"
					) {
					name: String
					age: Int
				}"#,
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
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common {
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "Fred" })),
                common: Common {
                    identity_seed: Some(1),
                    retry: true,
                    capture: captures([("doc1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc1")),
                common: Common {
                    capture: captures([("head1", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
						_commits(
							cid: [""#,
                    ),
                    Text::reference("head0"),
                    Text::from(r#"", ""#),
                    Text::reference("head1"),
                    Text::from(
                        r#""]
						) {
							cid
						}
					}"#,
                    ),
                ]),
                expected_data: json!({ "_commits": [{ "cid": m::r("head0") }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.cid.lookup")]
fn query_commits_with_first_commit_cid_for_field_commit_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Users(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"") { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_0", "/data/Users/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name": "Johnn" }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Users(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"") { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_1", "/data/Users/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
						_commits(
							cid: ""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(
                        r#""
						) {
							cid
						}
					}"#,
                    ),
                ]),
                expected_data: json!({ "_commits": [{ "cid": m::cid() }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.cid.lookup")]
fn query_commits_with_first_commit_cid_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { _commits(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"", filter: {fieldName: {_eq: "name"}}) { cid } }"#),
                ]),
                common: Common {
                    capture: captures([("FieldCID0_0_name_0", "/data/_commits/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "age": 22 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
						_commits(
							cid: ""#,
                    ),
                    Text::reference("FieldCID0_0_name_0"),
                    Text::from(
                        r#""
						) {
							cid
						}
					}"#,
                    ),
                ]),
                expected_data: json!({ "_commits": [{ "cid": m::cid() }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.cid.errors")]
fn query_commits_with_invalid_cid() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits(cid: "fhbnjfahfhfhanfhga") {
                                                cid
                                                height
                                                delta
                                            }
                                        }
                "#
                .into(),
                expected_error: Some("invalid cid".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.cid.errors")]
fn query_commits_with_invalid_short_cid() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits(cid: "bafybeidfhbnjfahfhfhanfhga") {
                                                cid
                                                height
                                                delta
                                            }
                                        }
                "#
                .into(),
                expected_error: Some("invalid cid".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.cid.errors")]
fn query_commits_with_unknown_cid() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        verified: Boolean
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits(cid: "bafybeid57gpbwi4i6bg7g35hhhhhhhhhhhhhhhhhhhhhhhdoesnotexist") {
                                                cid
                                                height
                                                delta
                                            }
                                        }
                "#.into(),
                expected_error: Some("cid either does not exist or belong to document".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
