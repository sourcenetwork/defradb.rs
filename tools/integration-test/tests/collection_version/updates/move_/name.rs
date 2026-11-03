//! Generated from the conformance scenarios of tests/integration/collection_version/updates/move/name_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.collection_name")]
fn col_version_update_move_name() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {}
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "name", "Kind": "String"} },
						{
							"op": "move",
							"from": "/Users/Name",
							"path": "/Users/Fields/1/Name"
						},
						{ "op": "replace", "path": "/Users/IsActive", "value": false }
					]
				"#
                .into(),
                expected_error: Some("collection name can't be empty".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
