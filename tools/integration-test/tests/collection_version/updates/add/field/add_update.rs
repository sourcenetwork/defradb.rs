//! Generated from the conformance scenarios of tests/integration/collection_version/updates/add/field/add_update_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, PatchCollection, Request, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.existing_docs")]
fn collection_version_updates_add_field_with_add_with_update_after_collection_update_and_commit_query(
) {
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
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "email": "ih8oraclelicensing@netscape.net" }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        _commits (filter: {fieldName: {_eq: "_C"}}) {
                                            collectionVersionId
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "collectionVersionId": "bafyreigvzkfdc4y2ppvvpmmdw3t7kv4nd5dgfh5jfytef3kbzem6po55zu",
                        },
                        {
                            "collectionVersionId": "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu",
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
#[mantra_macros::req_test("go.collection_version.patch.add_field.existing_docs")]
fn collection_version_updates_add_field_with_add_with_update_after_collection_update_and_version_join(
) {
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
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            _version {
                                                collectionVersionId
                                                fieldName
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Users": [
                        {
                            "_version": [
                                {
                                    "collectionVersionId": "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu",
                                    "fieldName": "_C",
                                },
                            ],
                            "name": "John",
                        },
                    ],
                }),
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
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "email": "ih8oraclelicensing@netscape.net" }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            email
                                            _version {
                                                collectionVersionId
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Users": [
                        {
                            "_version": [
                                {
                                    "collectionVersionId": "bafyreigvzkfdc4y2ppvvpmmdw3t7kv4nd5dgfh5jfytef3kbzem6po55zu",
                                },
                                {
                                    "collectionVersionId": "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu",
                                },
                            ],
                            "email": "ih8oraclelicensing@netscape.net",
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
