//! Generated from the conformance scenarios of tests/integration/index/patch_relation_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, ListIndexes, PatchCollection, SetActiveCollectionVersion,
};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.definition.one-to-one-patch")]
fn patch_relation_multiple_one_to_one_adds_unique_indexes_with_correct_i_ds() {
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
                    type Publisher {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{ "op": "add", "path": "/Book/Fields/-", "value": {
							"Name": "author", "Kind": "Author", "RelationName": "book_author", "IsPrimary": true
						}},
						{ "op": "add", "path": "/Book/Fields/-", "value": {
							"Name": "_authorID", "Kind": 1, "RelationName": "book_author", "IsPrimary": true
						}},
						{ "op": "add", "path": "/Author/Fields/-", "value": {
							"Name": "book", "Kind": "Book", "RelationName": "book_author"
						}},
						{ "op": "add", "path": "/Book/Fields/-", "value": {
							"Name": "publisher", "Kind": "Publisher", "RelationName": "book_publisher", "IsPrimary": true
						}},
						{ "op": "add", "path": "/Book/Fields/-", "value": {
							"Name": "_publisherID", "Kind": 1, "RelationName": "book_publisher", "IsPrimary": true
						}},
						{ "op": "add", "path": "/Publisher/Fields/-", "value": {
							"Name": "book", "Kind": "Book", "RelationName": "book_publisher"
						}}
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Book".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "Book__authorID_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "_authorID", "Descending": false })),
                                ])),
                            })),
                        })),
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "Book__publisherID_ASC",
                                "ID": 2,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "_publisherID", "Descending": false })),
                                ])),
                            })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Author".into(),
                expected_data: json!({ "indexes": m::unordered(json!([])) }),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Publisher".into(),
                expected_data: json!({ "indexes": m::unordered(json!([])) }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.one-to-one-patch")]
fn patch_relation_one_to_many_does_not_add_unique_index() {
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
            ListIndexes {
                collection: "Author".into(),
                expected_data: json!({ "indexes": m::unordered(json!([])) }),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Book".into(),
                expected_data: json!({ "indexes": m::unordered(json!([])) }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.one-to-one-patch")]
fn patch_relation_one_to_one_adds_unique_index() {
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
                "#.into(),
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
				"#.into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Author".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "Author__publishedID_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "_publishedID", "Descending": false })),
                                ])),
                            })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Book".into(),
                expected_data: json!({ "indexes": m::unordered(json!([])) }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.one-to-one-patch")]
fn patch_relation_one_to_one_with_version_switching_index_only_on_active_version() {
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
                "#.into(),
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
				"#.into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Author".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "Author__publishedID_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "_publishedID", "Descending": false })),
                                ])),
                            })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: "bafyreibvcavbxqwimz5vdxe5q5href63g3skc6ytg45hm4fqh6wsx57wmq".into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Author".into(),
                expected_data: json!({ "indexes": m::unordered(json!([])) }),
                ..Default::default()
            }
            .into(),
            SetActiveCollectionVersion {
                version_id: "bafyreihv2jdbz3sipc7tqdoycerkcjn6gehr5aleiroqlewvsmjd26unfq".into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Author".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "Author__publishedID_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "_publishedID", "Descending": false })),
                                ])),
                            })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
