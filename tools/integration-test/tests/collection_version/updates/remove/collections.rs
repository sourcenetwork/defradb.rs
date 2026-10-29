//! Generated from the conformance scenarios of tests/integration/collection_version/updates/remove/collections_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, DeleteDoc, GetCollections, PatchCollection, Request,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.version_chain")]
fn col_version_update_add_field_remove_multiple_new_collection_first_and_last() {
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
						{
							"op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11}
						}
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add", "path": "/Users/Fields/-", "value": {"Name": "postCode", "Kind": 11}
						}
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "remove",
							"path": "/bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu"
						},
						{
							"op": "remove",
							"path": "/Users"
						}
					]
				"#.into(),
                expected_error: Some("cannot delete a version that is used by a newer version, first delete the new version".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.version_chain")]
fn col_version_update_add_field_remove_multiple_new_collection_first_and_middle() {
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
						{
							"op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11}
						}
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add", "path": "/Users/Fields/-", "value": {"Name": "postCode", "Kind": 11}
						}
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "remove",
							"path": "/bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu"
						},
						{
							"op": "remove",
							"path": "/bafyreigvzkfdc4y2ppvvpmmdw3t7kv4nd5dgfh5jfytef3kbzem6po55zu"
						}
					]
				"#.into(),
                expected_error: Some("cannot delete a version that is used by a newer version, first delete the new version".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.version_chain")]
fn col_version_update_add_field_remove_multiple_new_collection_middle_and_last() {
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
							"op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11}
						}
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
							"op": "add", "path": "/Users/Fields/-", "value": {"Name": "postCode", "Kind": 11}
						}
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
							"op": "remove",
							"path": "/bafyreigvzkfdc4y2ppvvpmmdw3t7kv4nd5dgfh5jfytef3kbzem6po55zu"
						},
						{
							"op": "remove",
							"path": "/Users"
						}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                get_inactive: Some(true),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": false,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Users",
                            "PreviousVersion": null,
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
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.version_chain")]
fn col_version_update_add_field_remove_new_collection_and_activate_original() {
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
							"op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11}
						}
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
							"op": "remove",
							"path": "/Users"
						},
						{
							"op": "replace",
							"path": "/bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu/IsActive",
							"value": true
						}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                get_inactive: Some(true),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Users",
                            "PreviousVersion": null,
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
                            ],
                        })),
                    ],
                }),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.version_chain")]
fn col_version_update_add_field_remove_new_collection_different_patches() {
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
							"op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11}
						}
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
							"op": "remove",
							"path": "/Users"
						}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                get_inactive: Some(true),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": false,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Users",
                            "PreviousVersion": null,
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
                            ],
                        })),
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(r#"Cannot query field "Users" on type "Query"."#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.version_chain")]
fn col_version_update_add_field_remove_original_collection_different_patches() {
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
						{
							"op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11}
						}
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "remove",
							"path": "/bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu"
						}
					]
				"#.into(),
                expected_error: Some("cannot delete a version that is used by a newer version, first delete the new version".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.version_chain")]
fn col_version_update_add_field_remove_original_collection_same_patch() {
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
							"op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11}
						},
						{
							"op": "remove",
							"path": "/bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu"
						}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                get_inactive: Some(true),
                filtered: true,
                expected_data: json!({ "collections": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.version_chain")]
fn col_version_update_copy_collection_add_field_remove_original_collection() {
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
						{
							"op": "copy",
							"from": "/Users",
							"path": "/UsersV2"
						},
						{
							"op": "add", "path": "/UsersV2/Fields/-", "value": {"Name": "email", "Kind": 11}
						},
						{
							"op": "remove",
							"path": "/Users"
						}
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                get_inactive: Some(true),
                filtered: true,
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": false,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Users",
                            "PreviousVersion": null,
                        })),
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Users",
                            "PreviousVersion": {
                                "SourceCollectionID": "bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu",
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
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.relations")]
fn col_version_update_remove_both_related_collections_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        books: [Books]
                    }
                    type Books {
                        title: String
                        author: Users
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "remove", "path": "/Users" },
						{ "op": "remove", "path": "/Books" }
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                expected_data: json!({ "collections": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.relations")]
fn col_version_update_remove_collection_referenced_by_relation_other_side_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        books: [Books]
                    }
                    type Books {
                        title: String
                        author: Users
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "remove", "path": "/Books" }
					]
				"#
                .into(),
                expected_error: Some(
                    "cannot remove a collection while another field references it".into(),
                ),
                ..Default::default()
            }
            .into(),
            GetCollections {
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Books",
                            "PreviousVersion": null,
                        })),
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Users",
                            "PreviousVersion": null,
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
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.relations")]
fn col_version_update_remove_collection_referenced_by_relation_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        books: [Books]
                    }
                    type Books {
                        title: String
                        author: Users
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "remove", "path": "/Users" }
					]
				"#
                .into(),
                expected_error: Some(
                    "cannot remove a collection while another field references it".into(),
                ),
                ..Default::default()
            }
            .into(),
            GetCollections {
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Books",
                            "PreviousVersion": null,
                        })),
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Users",
                            "PreviousVersion": null,
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
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.basic")]
fn col_version_update_remove_collection_then_add_same_name_is_fresh_collection() {
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
                patch: r#"[{ "op": "remove", "path": "/Users" }]"#.into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        email: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            email
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(r#"Cannot query field "name" on type "Users"."#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.basic")]
fn col_version_update_remove_collection_then_add_same_shape_is_fresh_collection() {
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
                patch: r#"[{ "op": "remove", "path": "/Users" }]"#.into(),
                ..Default::default()
            }
            .into(),
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Alice" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.with_documents")]
fn col_version_update_remove_collection_with_data() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "remove",
							"path": "/Users"
						}
					]
				"#.into(),
                expected_error: Some("cannot delete a collection that has documents, first delete the documents and then delete the version".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.with_documents")]
fn col_version_update_remove_collection_with_soft_deleted_data() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "remove",
							"path": "/Users"
						}
					]
				"#.into(),
                expected_error: Some("cannot delete a collection that has documents, first delete the documents and then delete the version".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.basic")]
fn col_version_update_remove_collections_add_doc_to_removed_collection() {
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
							"op": "remove",
							"path": "/Users"
						}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                expected_error: Some("collection not found".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.basic")]
fn col_version_update_remove_collections_by_id() {
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
							"op": "remove",
							"path": "/bafyreiciz2hrrmt7ritk5gf5fyruw46v2tfhq5dc7qto4wgpzluben2smu"
						}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                filtered: true,
                expected_data: json!({ "collections": [] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                            add_Users(input:{}) {
                                                name
                                            }
                                        }
                "#
                .into(),
                expected_error: Some(
                    r#"Cannot query field "add_Users" on type "Mutation"."#.into(),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                            update_Users(input:{}) {
                                                name
                                            }
                                        }
                "#
                .into(),
                expected_error: Some(
                    r#"Cannot query field "update_Users" on type "Mutation"."#.into(),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(r#"Cannot query field "Users" on type "Query"."#.into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    subscription {
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(
                    r#"Cannot query field "Users" on type "Subscription"."#.into(),
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
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.basic")]
fn col_version_update_remove_collections_by_name() {
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
							"op": "remove",
							"path": "/Users"
						}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                filtered: true,
                expected_data: json!({ "collections": [] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                            add_Users(input:{}) {
                                                name
                                            }
                                        }
                "#
                .into(),
                expected_error: Some(
                    r#"Cannot query field "add_Users" on type "Mutation"."#.into(),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                            update_Users(input:{}) {
                                                name
                                            }
                                        }
                "#
                .into(),
                expected_error: Some(
                    r#"Cannot query field "update_Users" on type "Mutation"."#.into(),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(r#"Cannot query field "Users" on type "Query"."#.into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    subscription {
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(
                    r#"Cannot query field "Users" on type "Subscription"."#.into(),
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
#[mantra_macros::req_test("go.collection_version.patch.remove_collection.concurrency")]
fn col_version_update_remove_collections_concurrent_write() {
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
								"op": "remove",
								"path": "/Users"
							}
						]
					"#
                .into(),
                collection: Some("Users".into()),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            GetCollections {
                expected_data: json!({ "collections": [] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        _commits {
                                            cid
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "_commits": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
