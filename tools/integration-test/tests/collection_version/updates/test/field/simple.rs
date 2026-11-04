//! Generated from the conformance scenarios of tests/integration/collection_version/updates/test/field/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.test_op")]
fn collection_version_updates_test_field_errors() {
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
						{ "op": "test", "path": "/Users/Fields/1", "value": {"Name": "name", "Kind": 11} }
					]
				"#
                .into(),
                expected_error: Some("test failed".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.test_op")]
fn collection_version_updates_test_field_name_errors() {
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
						{ "op": "test", "path": "/Users/Fields/1/name", "value": "Email" }
					]
				"#
                .into(),
                expected_error: Some("failed: test failed".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.test_op")]
fn collection_version_updates_test_field_name_passes() {
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
						{ "op": "test", "path": "/Users/Fields/1/Name", "value": "name" }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.test_op")]
fn collection_version_updates_test_field_passes_targetting_kind_using_field_name_as_index() {
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
						{ "op": "test", "path": "/Users/Fields/name/Kind", "value": 11 }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.test_op")]
fn collection_version_updates_test_field_passes_using_field_name_as_index() {
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
            PatchCollection {
                patch: r#"
					[
						{ "op": "test", "path": "/Users/Fields/name", "value": {
							"FieldID": "bafyreiaezc5g33yzhyzcgbyiv476lovyztyoliotzksdfogoep5ktgpedq",
							"Name": "name", "Kind": 11, "Typ":1, "RelationName": null, "IsPrimary": false, "DefaultValue": null, "Size": 0
						} }
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.test_op")]
fn collection_version_updates_test_field_passes() {
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
            PatchCollection {
                patch: r#"
					[
						{ "op": "test", "path": "/Users/Fields/1", "value": {
							"FieldID": "bafyreiaezc5g33yzhyzcgbyiv476lovyztyoliotzksdfogoep5ktgpedq",
							"Name": "name", "Kind": 11, "Typ":1, "RelationName": null, "IsPrimary": false, "DefaultValue": null, "Size": 0
						} }
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
