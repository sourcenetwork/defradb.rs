//! Generated from the conformance scenarios of tests/integration/collection_version/updates/add/field/kind/foreign_object_array_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.kind.foreign_object")]
fn collection_version_updates_add_field_kind_foreign_object_array_known_collection() {
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {
							"Name": "foo", "Kind": "[Users]", "RelationName": "users_users"
						}}
					]
				"#
                .into(),
                expected_error: Some(
                    "relation missing field. Object: Users, RelationName: users_users".into(),
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
#[mantra_macros::req_test("go.collection_version.patch.add_field.kind.foreign_object")]
fn collection_version_updates_add_field_kind_foreign_object_array_no_relation_name() {
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {
							"Name": "foo", "Kind": "[Users]"
						}}
					]
				"#
                .into(),
                expected_error: Some("relation name cannot be empty. Field: foo".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.kind.foreign_object")]
fn collection_version_updates_add_field_kind_foreign_object_array_unknown_collection() {
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {
							"Name": "foo", "Kind": "[Unknown]"
						}}
					]
				"#
                .into(),
                expected_error: Some(
                    "no type found for given name. Field: foo, Kind: [Unknown]".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
