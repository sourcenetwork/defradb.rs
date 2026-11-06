//! Generated from the conformance scenarios of tests/integration/collection_version/updates/replace/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_collection")]
fn collection_version_updates_replace_collection_errors() {
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
						{
							"op": "replace", "path": "/Users", "value": {
								"Name": "Book",
								"Fields": [
									{"Name": "name", "Kind": 11}
								]
							}
						}
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
