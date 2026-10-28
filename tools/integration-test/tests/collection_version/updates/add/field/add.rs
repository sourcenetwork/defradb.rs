//! Generated from the conformance scenarios of tests/integration/collection_version/updates/add/field/add_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, PatchCollection, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.existing_docs")]
fn collection_version_updates_add_field_with_add_after_collection_update() {
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
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad", "email": "sqlizded@yahoo.ca" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            email
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "email": null, "name": "John" },
                        { "email": "sqlizded@yahoo.ca", "name": "Shahzad" },
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

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.existing_docs")]
fn collection_version_updates_add_field_with_add() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} }
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            _docID
                                            name
                                            email
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "_docID": m::r("doc0_0"), "email": null, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.existing_docs")]
fn collection_version_updates_add_field_with_existing_secondary_one_to_one_relationship() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        publisher: Publisher @primary
                    }

                    type Publisher {
                        book: Book
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{"op":"add","path":"/Publisher/Fields/-","value":{"Name":"name","Kind":"String"}}]"#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "Penguin Books" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Publisher {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Publisher": [{ "name": "Penguin Books" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
