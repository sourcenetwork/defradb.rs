//! Generated from the conformance scenarios of tests/integration/collection_version/updates/copy/field/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, PatchCollection, Request};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.copy_field")]
fn collection_version_updates_copy_field_and_replace_name_and_invalid_kind_substitution() {
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
            PatchCollection {
                patch: r#"
					[
						{ "op": "copy", "from": "/Users/Fields/1", "path": "/Users/Fields/2" },
						{ "op": "replace", "path": "/Users/Fields/2/Name", "value": "Age" },
						{ "op": "replace", "path": "/Users/Fields/2/Kind", "value": "NotAValidKind" }
					]
				"#
                .into(),
                expected_error: Some(
                    "no type found for given name. Field: Age, Kind: NotAValidKind".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.field_reorder")]
fn collection_version_updates_copy_field_errors_multiple() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        email: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "copy", "from": "/Users/Fields/1", "path": "/Users/Fields/2" },
						{ "op": "copy", "from": "/Users/Fields/1", "path": "/Users/Fields/2" }
					]
				"#.into(),
                expected_error: Some(r#"moving fields is not currently supported. Name: email, ProposedIndex: 2, ExistingIndex: 1
moving fields is not currently supported. Name: email, ProposedIndex: 3, ExistingIndex: 1
moving fields is not currently supported. Name: name, ProposedIndex: 4, ExistingIndex: 2
duplicate field. Name: email
duplicate field. Name: email"#.into()),
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
                "#.into(),
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.field_reorder")]
fn collection_version_updates_copy_field_errors() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        email: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "copy", "from": "/Users/Fields/1", "path": "/Users/Fields/2" }
					]
				"#
                .into(),
                expected_error: Some(
                    "moving fields is not currently supported. Name: email".into(),
                ),
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
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.copy_field")]
fn collection_version_updates_copy_field_with_and_replace_name() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        email: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "copy", "from": "/Users/Fields/1", "path": "/Users/Fields/3" },
						{ "op": "replace", "path": "/Users/Fields/3/Name", "value": "fax" },
						{ "op": "remove", "path": "/Users/Fields/3/FieldID" }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            email
                                            fax
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.copy_field")]
fn collection_version_updates_copy_field_with_replace_name_and_kind_substitution() {
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
            PatchCollection {
                patch: r#"
					[
						{ "op": "copy", "from": "/Users/Fields/1", "path": "/Users/Fields/2" },
						{ "op": "replace", "path": "/Users/Fields/2/Name", "value": "age" },
						{ "op": "replace", "path": "/Users/Fields/2/Kind", "value": "Int" },
						{ "op": "remove", "path": "/Users/Fields/2/FieldID" }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 3 })),
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
                                        Users {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "age": 3, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
