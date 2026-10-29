//! Generated from the conformance scenarios of tests/integration/collection_version/updates/add/branchable_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, GetCollections, PatchCollection, Request, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.branchable")]
fn branchable_collection_add_new_field_should_update_collection_definition() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/User/Fields/-", "value": {"Name": "email", "Kind": 11} }
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("User".into()),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": true,
                            "Name": "User",
                            "PreviousVersion": {
                                "SourceCollectionID": "bafyreibhpgygzsmki22sql5ejzcojrrxbc5iuhpydhdzxul5w2znc7zrgu",
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
                                    "Name": "email",
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

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.branchable")]
fn branchable_collection_add_new_field_with_multiple_docs_should_add_field() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @branchable {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/User/Fields/-", "value": {"Name": "email", "Kind": 11} }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "email": "andy@gmail.com", "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_1"),
                doc_map: json!({ "email": "islam@gmail.com" }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            name
                                            email
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        { "email": null, "name": "John" },
                        { "email": "islam@gmail.com", "name": "Islam" },
                        { "email": "andy@gmail.com", "name": "Andy" },
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
