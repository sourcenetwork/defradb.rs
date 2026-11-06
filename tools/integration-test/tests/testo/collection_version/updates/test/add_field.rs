//! Generated from the conformance scenarios of tests/integration/collection_version/updates/test/add_field_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.non_nillable")]
fn collection_version_updates_add_non_nillable_blob_field_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String }".into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "avatar", "Kind": 28} }]
				"#
                .into(),
                expected_error: Some(
                    "adding a non-nillable field to an existing collection is not supported".into(),
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
#[mantra_macros::req_test("go.collection_version.patch.add_field.non_nillable")]
fn collection_version_updates_add_non_nillable_bool_field_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String }".into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "verified", "Kind": 15} }]
				"#
                .into(),
                expected_error: Some(
                    "adding a non-nillable field to an existing collection is not supported".into(),
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
#[mantra_macros::req_test("go.collection_version.patch.add_field.non_nillable")]
fn collection_version_updates_add_non_nillable_date_time_field_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String }".into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "createdAt", "Kind": 27} }]
				"#
                .into(),
                expected_error: Some(
                    "adding a non-nillable field to an existing collection is not supported".into(),
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
#[mantra_macros::req_test("go.collection_version.patch.add_field.non_nillable")]
fn collection_version_updates_add_non_nillable_float32_field_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String }".into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "weight", "Kind": 25} }]
				"#
                .into(),
                expected_error: Some(
                    "adding a non-nillable field to an existing collection is not supported".into(),
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
#[mantra_macros::req_test("go.collection_version.patch.add_field.non_nillable")]
fn collection_version_updates_add_non_nillable_float64_field_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String }".into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "rating", "Kind": 24} }]
				"#
                .into(),
                expected_error: Some(
                    "adding a non-nillable field to an existing collection is not supported".into(),
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
#[mantra_macros::req_test("go.collection_version.patch.add_field.non_nillable")]
fn collection_version_updates_add_non_nillable_int_field_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String }".into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "score", "Kind": 23} }]
				"#
                .into(),
                expected_error: Some(
                    "adding a non-nillable field to an existing collection is not supported".into(),
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
#[mantra_macros::req_test("go.collection_version.patch.add_field.non_nillable")]
fn collection_version_updates_add_non_nillable_json_field_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String }".into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "metadata", "Kind": 29} }]
				"#
                .into(),
                expected_error: Some(
                    "adding a non-nillable field to an existing collection is not supported".into(),
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
#[mantra_macros::req_test("go.collection_version.patch.add_field.non_nillable")]
fn collection_version_updates_add_non_nillable_string_field_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String }".into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 26} }]
				"#
                .into(),
                expected_error: Some(
                    "adding a non-nillable field to an existing collection is not supported".into(),
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
#[mantra_macros::req_test("go.collection_version.patch.test_op")]
fn collection_version_updates_test_add_field_blocked_by_test() {
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
						{ "op": "test", "path": "/Users/Name", "value": "Author" },
						{ "op": "add", "path": "/Users/Fields/-", "value": {"name": "Email", "Kind": 11} }
					]
				"#
                .into(),
                expected_error: Some("test failed".into()),
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
                expected_error: Some(r#"Cannot query field "email" on type "Users""#.into()),
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
fn collection_version_updates_test_add_field() {
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
						{ "op": "test", "path": "/Users/Name", "value": "Users" },
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} }
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
