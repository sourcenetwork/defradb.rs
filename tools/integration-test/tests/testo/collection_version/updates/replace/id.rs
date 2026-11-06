//! Generated from the conformance scenarios of tests/integration/collection_version/updates/replace/id_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, PatchCollection};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.immutable_identity")]
fn col_version_update_replace_id_with_empty_errors() {
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
							"path": "/bafyreihuyovjl5ezgpud5xyqnouzsgx25x3ssrx3ncdv5p3guocc3laqna/VersionID",
							"value": ""
						}
					]
				"#
                .into(),
                expected_error: Some("collection ID cannot be empty".into()),
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
fn col_version_update_replace_id_with_existing_different_root_errors() {
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
            AddCollection {
                sdl: r#"
                    type Dogs {}
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
							"path": "/bafyreihuyovjl5ezgpud5xyqnouzsgx25x3ssrx3ncdv5p3guocc3laqna/VersionID",
							"value": "bafyreiguy7x6zs57dgbpuduiacckubvkbgi6bo2oaytu5dlthr2hsmawxu"
						},
						{
							"op": "replace",
							"path": "/bafyreiguy7x6zs57dgbpuduiacckubvkbgi6bo2oaytu5dlthr2hsmawxu/VersionID",
							"value": "bafyreihuyovjl5ezgpud5xyqnouzsgx25x3ssrx3ncdv5p3guocc3laqna"
						}
					]
				"#
                .into(),
                expected_error: Some("collection source must belong to host collection.".into()),
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
fn col_version_update_replace_id_with_existing_errors() {
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
							"path": "/Users/VersionID",
							"value": "invalid cid"
						}
					]
				"#
                .into(),
                expected_error: Some(
                    "invalid cid: selected encoding not supported. VersionID: invalid cid".into(),
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
#[mantra_macros::req_test("go.collection_version.patch.immutable_identity")]
fn col_version_update_replace_id_with_existing_same_root_errors() {
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "name", "Kind": "String"} }
					]
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
							"path": "/bafyreihuyovjl5ezgpud5xyqnouzsgx25x3ssrx3ncdv5p3guocc3laqna/VersionID",
							"value": "bafyreieqhzanpek5ssb7ofi3qelbvl2nwh6s7x3w2mlzbcnqaqol3elltq"
						},
						{
							"op": "replace",
							"path": "/bafyreieqhzanpek5ssb7ofi3qelbvl2nwh6s7x3w2mlzbcnqaqol3elltq/VersionID",
							"value": "bafyreihuyovjl5ezgpud5xyqnouzsgx25x3ssrx3ncdv5p3guocc3laqna"
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

#[test]
#[mantra_macros::req_test("go.collection_version.patch.immutable_identity")]
fn col_version_update_replace_id_with_new_errors() {
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
							"path": "/Users/VersionID",
							"value": "bafkreibifvyfr6qvb6wx4v4cogvcdksb3v7vniaon7hdzzqb62cotpmlc4"
						}
					]
				"#
                .into(),
                expected_error: Some(
                    "unknown CID, collection ids cannot be manually defined".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
