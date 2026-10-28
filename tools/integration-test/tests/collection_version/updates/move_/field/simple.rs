//! Generated from the conformance scenarios of tests/integration/collection_version/updates/move/field/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.field_reorder")]
fn collection_version_updates_move_field_errors_multiple() {
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
						{ "op": "move", "from": "/Users/Fields/1", "path": "/Users/Fields/-" }
					]
				"#.into(),
                expected_error: Some(r#"moving fields is not currently supported. Name: name, ProposedIndex: 1, ExistingIndex: 2
moving fields is not currently supported. Name: email, ProposedIndex: 2, ExistingIndex: 1"#.into()),
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
fn collection_version_updates_move_field_errors() {
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
						{ "op": "move", "from": "/Users/Fields/1", "path": "/Users/Fields/-" }
					]
				"#.into(),
                expected_error: Some("moving fields is not currently supported. Name: name, ProposedIndex: 1, ExistingIndex: 2".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
