//! Generated from the conformance scenarios of tests/integration/acp/dac/branchable/collection_commits_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDACActorRelationship, AddDACPolicy, AddDoc, GetCollections, Request,
    WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.branchable.commits")]
fn acp_branchable_collection_commits_collection_reader_still_gated_by_private_doc() {
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
	type Users @branchable @policy(
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
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Users".into()),
                filtered: true,
                common: Common {
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
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
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_0"),
                    Text::reference("collection0"),
                ]),
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(2),
                collection: "Users".into(),
                doc_id: Text::reference("collection0"),
                relation: "reader".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            cid
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([{ "cid": m::unique("collectionCid") }])),
                }),
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
#[mantra_macros::req_test("go.acp.dac.branchable.commits")]
fn acp_branchable_collection_commits_no_identity_cannot_see_any_commit() {
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
	type Users @branchable @policy(
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
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Users".into()),
                filtered: true,
                common: Common {
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
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
                doc_map: Some(json!({ "name": "Shahzad", "age": 28 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_0"),
                    Text::reference("collection0"),
                ]),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            cid
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "_commits": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.branchable.commits")]
fn acp_branchable_collection_commits_owner_can_see_all_commits() {
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
	type Users @branchable @policy(
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
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Users".into()),
                filtered: true,
                common: Common {
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
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
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_0"),
                    Text::reference("collection0"),
                ]),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            cid
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        { "cid": m::unique("uniqueCid") },
                        { "cid": m::unique("uniqueCid") },
                        { "cid": m::unique("uniqueCid") },
                        { "cid": m::unique("uniqueCid") },
                    ])),
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
#[mantra_macros::req_test("go.acp.dac.branchable.commits")]
fn acp_branchable_collection_commits_public_collection_is_visible_to_all() {
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
	type Users @branchable @policy(
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
            GetCollections {
                collection_name: Some("Users".into()),
                filtered: true,
                common: Common {
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
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
                doc_map: Some(json!({ "name": "Shahzad", "age": 28 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_0"),
                    Text::reference("collection0"),
                ]),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            cid
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        { "cid": m::unique("uniqueCid") },
                        { "cid": m::unique("uniqueCid") },
                        { "cid": m::unique("uniqueCid") },
                        { "cid": m::unique("uniqueCid") },
                    ])),
                }),
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
#[mantra_macros::req_test("go.acp.dac.branchable.commits")]
fn acp_branchable_collection_commits_shared_collection_reader_can_see_all_commits() {
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
	type Users @branchable @policy(
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
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Users".into()),
                filtered: true,
                common: Common {
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
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
                doc_map: Some(json!({ "name": "Shahzad", "age": 28 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_0"),
                    Text::reference("collection0"),
                ]),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            cid
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "_commits": [] }),
                common: Common {
                    identity_seed: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDACActorRelationship {
                target_identity_seed: Some(2),
                collection: "Users".into(),
                doc_id: Text::reference("collection0"),
                relation: "reader".into(),
                expected_data: json!({ "existedAlready": false }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            cid
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        { "cid": m::unique("afterCid") },
                        { "cid": m::unique("afterCid") },
                        { "cid": m::unique("afterCid") },
                        { "cid": m::unique("afterCid") },
                    ])),
                }),
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
#[mantra_macros::req_test("go.acp.dac.branchable.commits")]
fn acp_branchable_collection_commits_stranger_cannot_see_any_commit() {
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
	type Users @branchable @policy(
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
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Users".into()),
                filtered: true,
                common: Common {
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
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
                doc_map: Some(json!({ "name": "Shahzad", "age": 28 })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![
                    Text::reference("doc0_0"),
                    Text::reference("collection0"),
                ]),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            cid
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        { "cid": m::unique("ownerCid") },
                        { "cid": m::unique("ownerCid") },
                        { "cid": m::unique("ownerCid") },
                        { "cid": m::unique("ownerCid") },
                    ])),
                }),
                common: Common {
                    identity_seed: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            cid
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "_commits": [] }),
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
