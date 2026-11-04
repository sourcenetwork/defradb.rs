//! Generated from the conformance scenarios of tests/integration/collection_version/updates/replace/sources_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.immutable_identity")]
fn col_version_update_replace_sources_errors() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {}
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "replace",
							"path": "/Users/PreviousVersion",
							"value": {"SourceCollectionID": "bafyreihuyovjl5ezgpud5xyqnouzsgx25x3ssrx3ncdv5p3guocc3laqna"}
						}
					]
				"#.into(),
                expected_error: Some("collection sources cannot be added or removed.".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.immutable_identity")]
fn col_version_update_replace_sources_with_query_source_errors() {
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
						{
							"op": "replace",
							"path": "/Users/Query",
							"value": {"Query": {"Name": "Users"}}
						}
					]
				"#
                .into(),
                expected_error: Some("collection sources cannot be added or removed.".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
