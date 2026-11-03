//! Generated from the conformance scenarios of tests/integration/query/simple/with_cid_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, DeleteDoc, Request, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.cid.list")]
fn query_simple_duplicate_ci_ds_for_same_doc() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
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
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_0", "/data/Users/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Users(docID: ""#),
                    Text::reference("doc0_1"),
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_1_0", "/data/Users/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (
							cid: [""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(r#"", ""#),
                    Text::reference("CID0_0_0"),
                    Text::from(
                        r#""]
						) {
						name
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.cid.list")]
fn query_simple_list_of_one_cid() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
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
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
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
					Users (
							cid: [""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(
                        r#""]
						) {
						name
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.cid.list")]
fn query_simple_multiple_ci_ds_for_same_doc() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
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
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
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
                doc_map: json!({ "name": "Johnnn" }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Users(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_1", "/data/Users/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Users(docID: ""#),
                    Text::reference("doc0_1"),
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_1_0", "/data/Users/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (
							cid: [""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(r#"", ""#),
                    Text::reference("CID0_0_1"),
                    Text::from(
                        r#""]
						) {
						name
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "John" }, { "name": "Johnnn" }] }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.cid.list")]
fn query_simple_multiple_ci_ds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
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
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_0", "/data/Users/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Users(docID: ""#),
                    Text::reference("doc0_1"),
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_1_0", "/data/Users/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Users(docID: ""#),
                    Text::reference("doc0_2"),
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_2_0", "/data/Users/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (
							cid: [""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(r#"", ""#),
                    Text::reference("CID0_1_0"),
                    Text::from(
                        r#""]
						) {
						name
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "John" }, { "name": "Fred" }] }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.cid.errors")]
fn query_simple_unknown_cid() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users (
                                                cid: "bafybeid57gpbwi4i6bg7g35hhhhhhhhhhhhhhhhhhhhhhhdoesnotexist"
                                            ) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_error: Some("failed to get block in blockstore: ipld: could not find".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.select.show-deleted")]
fn query_simple_with_cid_after_delete_operation_should_return_user() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
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
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_0", "/data/Users/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Users(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
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
					Users (
						cid: ""#,
                    ),
                    Text::reference("CID0_0_1"),
                    Text::from(
                        r#""
						showDeleted: true
					){
						name
						_deleted
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "_deleted": true, "name": "John" }] }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.cid.errors")]
fn query_simple_with_cid_from_another_collection_returns_empty() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                    type Pets {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
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
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
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
					Pets (
							cid: ""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(
                        r#""
						) {
						name
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Pets": [] }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.cid.doc-version")]
fn query_simple_with_cid_multiple_docs() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
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
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_0", "/data/Users/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { Users(docID: ""#),
                    Text::reference("doc0_1"),
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_1_0", "/data/Users/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (
							cid: ""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(
                        r#""
						) {
						name
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.cid.doc-version")]
fn query_simple_with_cid() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
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
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
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
					Users (
							cid: ""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(
                        r#""
						) {
						name
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.cid.counter")]
fn query_simple_with_cid_and_counter_after_update_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        counter: Int @crdt(type: pcounter)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "counter": 1 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { User(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_0", "/data/User/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "counter": 1 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"query { User(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"", showDeleted: true) { _version { cid } } }"#),
                ]),
                common: Common {
                    capture: captures([("CID0_0_1", "/data/User/0/_version/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					User(cid: ""#,
                    ),
                    Text::reference("CID0_0_1"),
                    Text::from(
                        r#"") {
						counter
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "User": [{ "counter": 2 }] }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.cid.errors")]
fn query_simple_with_invalid_cid() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 21 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users (cid: "any non-nil string value - this will be ignored") {
                                            Name
                                        }
                                    }
                "#.into(),
                expected_error: Some("invalid cid: selected encoding not supported".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
