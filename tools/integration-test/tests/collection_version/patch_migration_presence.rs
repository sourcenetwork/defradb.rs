//! Generated from the conformance scenarios of tests/integration/collection_version/patch_migration_presence_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::Lens;
use integration_test::actions::{AddCollection, GetCollections, PatchCollection};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.patch_transform")]
fn collection_version_patch_none_migration_no_transform_recorded() {
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "name", "Kind": "String"} }
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Users".into()),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Users",
                            "PreviousVersion": {
                                "SourceCollectionID": "bafyreihuyovjl5ezgpud5xyqnouzsgx25x3ssrx3ncdv5p3guocc3laqna",
                                "Transform": null,
                            },
                        })),
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.migrations.patch_transform")]
fn collection_version_patch_some_empty_migration_transform_recorded() {
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "name", "Kind": "String"} }
					]
				"#.into(),
                lens: Some(Lens {
                    ..Default::default()
                }),
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Users".into()),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Users",
                            "PreviousVersion": {
                                "SourceCollectionID": "bafyreihuyovjl5ezgpud5xyqnouzsgx25x3ssrx3ncdv5p3guocc3laqna",
                                "Transform": "bafyreiagp6nl5qdyhlqvwgj3l2dvxz6nqte2btv7i7qhhjgatufehkvmka",
                            },
                        })),
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
