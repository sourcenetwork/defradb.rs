//! Generated from the conformance scenarios of tests/integration/collection_version/updates/add/collections_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_collection")]
fn col_version_update_add_collections_errors() {
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
							"op": "add",
							"path": "/hgfgsagasga",
							"value": {"VersionID": "hgfgsagasga", "Name": "Dogs"}
						}
					]
				"#
                .into(),
                expected_error: Some("adding collections via patch is not supported.".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_collection")]
fn col_version_update_add_collections_with_empty_id_errors() {
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
						{ "op": "add", "path": "/hgfgsagasga", "value": {"VersionID": "", "Name": "Dogs"} }
					]
				"#
                .into(),
                expected_error: Some("adding collections via patch is not supported".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_collection")]
fn col_version_update_add_collections_with_no_index_errors() {
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
						{ "op": "add", "path": "/-", "value": {"Name": "Dogs"} }
					]
				"#
                .into(),
                expected_error: Some("adding collections via patch is not supported.".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_collection")]
fn col_version_update_add_collections_with_undefined_id_errors() {
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
						{ "op": "add", "path": "/hgfgsagasga", "value": {"Name": "Dogs"} }
					]
				"#
                .into(),
                expected_error: Some("adding collections via patch is not supported".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
