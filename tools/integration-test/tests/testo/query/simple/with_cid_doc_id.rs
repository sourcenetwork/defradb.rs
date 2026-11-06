//! Generated from the conformance scenarios of tests/integration/query/simple/with_cid_doc_id_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.cid.counter")]
fn cid_and_doc_id_query_contains_p_counter_with_float_kind_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        points: Float @crdt(type: pcounter)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": 10.2 })),
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
                doc_map: json!({ "points": 20.6 }),
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
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (
						cid: ""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(
                        r#"",
						docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#""
					) {
						name
						points
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "John", "points": 10.2 }] }),
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
fn cid_and_doc_id_query_contains_p_counter_with_int_kind_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        points: Int @crdt(type: pcounter)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": 10 })),
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
                doc_map: json!({ "points": 20 }),
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
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (
						cid: ""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(
                        r#"",
						docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#""
					) {
						name
						points
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "John", "points": 10 }] }),
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
fn cid_and_doc_id_query_contains_pn_counter_with_float_kind_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        points: Float @crdt(type: pncounter)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": 10.2 })),
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
                doc_map: json!({ "points": -5.3 }),
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
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "points": 20.6 }),
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
                    capture: captures([("CID0_0_2", "/data/Users/0/_version/0/cid")]),
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
                        r#"",
						docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#""
					) {
						name
						points
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "John", "points": 10.2 }] }),
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
fn cid_and_doc_id_query_contains_pn_counter_with_int_kind_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        points: Int @crdt(type: pncounter)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "points": 10 })),
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
                doc_map: json!({ "points": -5 }),
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
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "points": 20 }),
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
                    capture: captures([("CID0_0_2", "/data/Users/0/_version/0/cid")]),
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
                        r#"",
						docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#""
					) {
						name
						points
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "John", "points": 10 }] }),
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
fn query_simple_with_cid_and_doc_id() {
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
                        r#"",
							docID: ""#,
                    ),
                    Text::reference("doc0_0"),
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
#[mantra_macros::req_test("go.query.simple.cid.errors")]
fn query_simple_with_invalid_cid_and_invalid_doc_id() {
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
                                                cid: "any non-nil string value - this will be ignored",
                                                docID: "invalid docID"
                                            ) {
                                            name
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

#[test]
#[mantra_macros::req_test("go.query.simple.cid.errors")]
fn query_simple_with_unknown_cid_and_invalid_doc_id() {
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
                                                cid: "bafybeid57gpbwi4i6bg7g357vwwyzsmr4bjo22rmhoxrwqvdxlqxcgaqvu",
                                                docID: "invalid docID"
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
#[mantra_macros::req_test("go.query.simple.cid.doc-version")]
fn query_simple_with_update_and_first_cid_and_doc_id() {
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
                doc_map: json!({ "name": "Johnn" }),
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
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (
							cid: ""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(
                        r#"",
							docID: ""#,
                    ),
                    Text::reference("doc0_0"),
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
fn query_simple_with_update_and_first_cid_and_doc_id_and_schema_version() {
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
                    Text::from(r#"") { _version { collectionVersionId } } }"#),
                ]),
                common: Common {
                    capture: captures([(
                        "CollectionVersionID0",
                        "/data/Users/0/_version/0/collectionVersionId",
                    )]),
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
					Users (
							cid: ""#,
                    ),
                    Text::reference("CID0_0_0"),
                    Text::from(
                        r#"",
							docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#""
						) {
						name
						_version {
							collectionVersionId
						}
					}
				}"#,
                    ),
                ]),
                expected_data: json!({
                    "Users": [
                        {
                            "_version": [{ "collectionVersionId": m::r("CollectionVersionID0") }],
                            "name": "John",
                        },
                    ],
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
#[mantra_macros::req_test("go.query.simple.cid.doc-version")]
fn query_simple_with_update_and_last_cid_and_doc_id() {
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
                doc_map: json!({ "name": "Johnn" }),
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
            Request {
                request: Text::concat([
                    Text::from(
                        r#"query {
					Users (
							cid: ""#,
                    ),
                    Text::reference("CID0_0_1"),
                    Text::from(
                        r#"",
							docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#""
						) {
						name
					}
				}"#,
                    ),
                ]),
                expected_data: json!({ "Users": [{ "name": "Johnn" }] }),
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
fn query_simple_with_update_and_middle_cid_and_doc_id() {
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
                doc_map: json!({ "name": "Johnn" }),
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
                    capture: captures([("CID0_0_2", "/data/Users/0/_version/0/cid")]),
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
                        r#"",
							docID: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(
                        r#""
						) {
						name
						_version {
							cid
						}
					}
				}"#,
                    ),
                ]),
                expected_data: json!({
                    "Users": [
                        {
                            "_version": [{ "cid": m::r("CID0_0_1") }, { "cid": m::r("CID0_0_0") }],
                            "name": "Johnn",
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
