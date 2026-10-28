//! Generated from the conformance scenarios of tests/integration/collection_version/updates/add/field/crdt/string_typ_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, GetCollectionsFiltered, PatchCollection};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.crdt.valid")]
fn collection_version_updates_add_field_crdt_typ_as_string_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "foo", "Kind": "Int", "Typ": "pncounter"} }
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            GetCollectionsFiltered {
                collection_name: Some("Users".into()),
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Users",
                            "PreviousVersion": {
                                "SourceCollectionID": "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu",
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
                                    "Name": "name",
                                    "Kind": 11,
                                    "Typ": 1,
                                    "IsPrimary": false,
                                    "DefaultValue": null,
                                    "RelationName": null,
                                    "Size": 0,
                                })),
                                m::object(json!({
                                    "Name": "foo",
                                    "Kind": 4,
                                    "Typ": 4,
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

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.crdt.invalid")]
fn collection_version_updates_add_field_crdt_typ_as_unknown_string_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "foo", "Kind": "Int", "Typ": "notacrdt"} }
					]
				"#.into(),
                expected_error: Some("unknown crdt string representation".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
