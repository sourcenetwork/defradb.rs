//! Generated from the conformance scenarios of tests/integration/collection_version/updates/replace/policy_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.immutable_settings")]
fn col_version_update_replace_policy_errors() {
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
							"path": "/bafyreihuyovjl5ezgpud5xyqnouzsgx25x3ssrx3ncdv5p3guocc3laqna/Policy",
							"value": {}
						}
					]
				"#
                .into(),
                expected_error: Some("collection policy cannot be mutated.".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.immutable_settings")]
fn col_version_update_replace_policy_id_errors() {
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
							"path": "/bafyreihuyovjl5ezgpud5xyqnouzsgx25x3ssrx3ncdv5p3guocc3laqna/Policy",
							"value": {"ID": "dfe202ffb4f0fe9b46157c313213a383"}
						}
					]
				"#
                .into(),
                expected_error: Some("collection policy cannot be mutated.".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.immutable_settings")]
fn col_version_update_replace_policy_resource_errors() {
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
							"path": "/bafyreihuyovjl5ezgpud5xyqnouzsgx25x3ssrx3ncdv5p3guocc3laqna/Policy",
							"value": {"ResourceName": "mutatingResource"}
						}
					]
				"#
                .into(),
                expected_error: Some("collection policy cannot be mutated.".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
