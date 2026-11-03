//! Generated from the conformance scenarios of tests/integration/collection_version/updates/add/field/crdt/object_bool_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.crdt.invalid")]
fn collection_version_updates_add_field_crdt_object_with_bool_field_errors_multiple() {
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "foo", "Kind": 2, "Typ":2} },
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "bar", "Kind": 2, "Typ":2} }
					]
				"#
                .into(),
                expected_error: Some(
                    r#"CRDT type not supported. Name: foo, CRDTType: object
CRDT type not supported. Name: bar"#
                        .into(),
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
#[mantra_macros::req_test("go.collection_version.patch.add_field.crdt.invalid")]
fn collection_version_updates_add_field_crdt_object_with_bool_field_errors() {
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "foo", "Kind": 2, "Typ":2} }
					]
				"#
                .into(),
                expected_error: Some("CRDT type not supported. Name: foo, CRDTType: object".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
