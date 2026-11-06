//! Generated from the conformance scenarios of tests/integration/collection_version/updates/add/field/with_relation_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.basic")]
fn collection_version_updates_add_field_does_not_affect_existing_relation() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        author: Author
                    }

                    type Author {
                        name: String
                        books: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Book/Fields/-", "value": {"Name": "rating", "Kind": 4} }
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
