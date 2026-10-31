//! Generated from the conformance scenarios of tests/integration/collection_version/updates/copy/name_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, GetCollections, PatchCollection};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.collection_name")]
fn col_version_update_copy_name() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        fullName: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Books {}
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Books/Fields/-", "value": {"Name": "name", "Kind": "String"} },
						{
							"op": "copy",
							"from": "/Users/Fields/1/Name",
							"path": "/Books/Fields/1/Name"
						},
						{ "op": "replace", "path": "/Users/IsActive", "value": false }
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Books".into()),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Books",
                            "PreviousVersion": {
                                "SourceCollectionID": "bafyreia5nnmpybnn3yodhafe5cydbov3zhhyaz2ovez35uweglxkp4kopm",
                                "Transform": null,
                            },
                            "Fields": [
                                m::object(json!({
                                    "Name": "_docID",
                                    "Kind": 1,
                                    "Typ": 0,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "fullName",
                                    "Kind": 11,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                            ],
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
