//! Generated from the conformance scenarios of tests/integration/collection_version/updates/add/field/kind/foreign_object_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, PatchCollection, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.kind.foreign_object")]
fn collection_version_updates_add_field_kind_foreign_object_id_field_invalid_kind() {
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {
							"Name": "foo", "Kind": "Users"
						}},
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "_fooID", "Kind": 2} }
					]
				"#.into(),
                expected_error: Some("relational id field of invalid kind. Field: _fooID, Expected: ID, Actual: Boolean".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.kind.foreign_object")]
fn collection_version_updates_add_field_kind_foreign_object_id_field_missing_kind() {
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {
							"Name": "foo", "Kind": "Users"
						}},
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "_fooID"} }
					]
				"#
                .into(),
                expected_error: Some(
                    "relational id field of invalid kind. Field: _fooID, Expected: ID, Actual: 0"
                        .into(),
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
#[mantra_macros::req_test("go.collection_version.patch.add_field.kind.foreign_object")]
fn collection_version_updates_add_field_kind_foreign_object_succeeds() {
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {
							"Name": "foo", "Kind": "Users", "RelationName": "users_users", "IsPrimary": true
						}},
						{ "op": "add", "path": "/Users/Fields/-", "value": {
							"Name": "_fooID", "Kind": 1, "RelationName": "users_users", "IsPrimary": true
						}}
					]
				"#
                .into(),
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "foo": m::r("doc0_0"), "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
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
                                            foo {
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "foo": null, "name": "John" },
                        { "foo": { "name": "John" }, "name": "Keenan" },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.add_field.kind.foreign_object")]
fn collection_version_updates_add_field_kind_foreign_object_unknown_collection() {
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {
							"Name": "foo", "Kind": "Unknown"
						}}
					]
				"#
                .into(),
                expected_error: Some(
                    "no type found for given name. Field: foo, Kind: Unknown".into(),
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
#[mantra_macros::req_test("go.collection_version.patch.add_field.kind.foreign_object")]
fn collection_version_updates_add_field_kind_foreign_object_with_patch_adding_one_to_many_relation_in_same_batch_should_succeed(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                    }
                    type Book {
                        title: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Author/Fields/-", "value": {
							"Name": "published", "Kind": "[Book]", "RelationName": "author_book"
						}},
						{ "op": "add", "path": "/Book/Fields/-", "value": {
							"Name": "author", "Kind": "Author", "RelationName": "author_book", "IsPrimary": true
						}},
						{ "op": "add", "path": "/Book/Fields/-", "value": {
							"Name": "_authorID", "Kind": 1, "RelationName": "author_book", "IsPrimary": true
						}}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "F. Scott Fitzgerald" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "The Great Gatsby" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Tender Is the Night" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Author {
                                            name
                                            published {
                                                title
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "F. Scott Fitzgerald",
                            "published": [
                                { "title": "The Great Gatsby" },
                                { "title": "Tender Is the Night" },
                            ],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book {
                                            title
                                            author {
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "author": { "name": "F. Scott Fitzgerald" },
                            "title": "The Great Gatsby",
                        },
                        {
                            "author": { "name": "F. Scott Fitzgerald" },
                            "title": "Tender Is the Night",
                        },
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
#[mantra_macros::req_test("go.collection_version.patch.add_field.kind.foreign_object")]
fn collection_version_updates_add_field_kind_foreign_object_with_patch_adding_one_to_one_relation_in_same_batch_should_succeed(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                    }
                    type Book {
                        title: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Author/Fields/-", "value": {
							"Name": "published", "Kind": "Book", "RelationName": "author_book", "IsPrimary": true
						}},
						{ "op": "add", "path": "/Author/Fields/-", "value": {
							"Name": "_publishedID", "Kind": 1, "RelationName": "author_book", "IsPrimary": true
						}},
						{ "op": "add", "path": "/Book/Fields/-", "value": {
							"Name": "author", "Kind": "Author", "RelationName": "author_book"
						}}
					]
				"#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "The Great Gatsby" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(
                    json!({ "name": "F. Scott Fitzgerald", "published": m::r("doc1_0") }),
                ),
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
                                        Author {
                                            name
                                            published {
                                                title
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "F. Scott Fitzgerald",
                            "published": { "title": "The Great Gatsby" },
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book {
                                            title
                                            author {
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "author": { "name": "F. Scott Fitzgerald" },
                            "title": "The Great Gatsby",
                        },
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
#[mantra_macros::req_test("go.collection_version.patch.add_field.kind.foreign_object")]
fn collection_version_updates_add_field_kind_foreign_object() {
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
						{ "op": "add", "path": "/Users/Fields/-", "value": {"Name": "foo", "Kind": 16} }
					]
				"#
                .into(),
                expected_error: Some("no type found for given name. Type: 16".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
