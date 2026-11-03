//! Generated from the conformance scenarios of tests/integration/collection_version/updates/copy/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_collection")]
fn collection_version_updates_copy_collection_with_remove_id_and_replace_name() {
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
						{ "op": "copy", "from": "/Users", "path": "/Book" },
						{ "op": "remove", "path": "/Book/CollectionID" },
						{ "op": "remove", "path": "/Book/VersionID" },
						{ "op": "replace", "path": "/Book/Name", "value": "Book" }
					]
				"#
                .into(),
                expected_error: Some(
                    "adding collections via patch is not supported. Name: Book".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
