//! Generated from the conformance scenarios of tests/integration/backup/self_reference/import_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, ExportBackup, ImportBackup, NewNode, Request, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.backup.import.relations")]
fn backup_self_ref_import_primary_relation_with_second_collection_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        book: Book @relation(name: "author_book")
                        reviewed: Book @relation(name: "reviewedBy_reviewed")
                    }
                    type Book {
                        name: String
                        author: Author @primary @relation(name: "author_book")
                        reviewedBy: Author @primary @relation(name: "reviewedBy_reviewed")
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            ImportBackup {
                import_content: r#"{
					"Author":[
						{
							"_docID":"bae-ca99414a-8336-537d-87d7-a7c4d90903b4",
							"_docIDNew":"bae-ca99414a-8336-537d-87d7-a7c4d90903b4",
							"name":"John"
						}
					],
					"Book":[
						{
							"name":"John and the sourcerers' stone",
							"author":"bae-ca99414a-8336-537d-87d7-a7c4d90903b4",
							"reviewedBy":"bae-ca99414a-8336-537d-87d7-a7c4d90903b4"
						}
					]
				}"#
                .into(),
                missing_file: false,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Book {
                            name
                            author {
                                name
                                reviewed {
                                    name
                                }
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "author": {
                                "name": "John",
                                "reviewed": { "name": "John and the sourcerers' stone" },
                            },
                            "name": "John and the sourcerers' stone",
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
#[mantra_macros::req_test("go.backup.import.relations")]
fn backup_self_ref_import_primary_relation_with_second_collection_wrong_order_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        book: Book @relation(name: "author_book")
                        reviewed: Book @relation(name: "reviewedBy_reviewed")
                    }
                    type Book {
                        name: String
                        author: Author @primary @relation(name: "author_book")
                        reviewedBy: Author @primary @relation(name: "reviewedBy_reviewed")
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            ImportBackup {
                import_content: r#"{
					"Book":[
						{
							"name":"John and the sourcerers' stone",
							"author":"bae-ca99414a-8336-537d-87d7-a7c4d90903b4",
							"reviewedBy":"bae-ca99414a-8336-537d-87d7-a7c4d90903b4"
						}
					],
					"Author":[
						{
							"_docID":"bae-ca99414a-8336-537d-87d7-a7c4d90903b4",
							"_docIDNew":"bae-ca99414a-8336-537d-87d7-a7c4d90903b4",
							"name":"John"
						}
					]
				}"#
                .into(),
                missing_file: false,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Book {
                            name
                            author {
                                name
                                reviewed {
                                    name
                                }
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "author": {
                                "name": "John",
                                "reviewed": { "name": "John and the sourcerers' stone" },
                            },
                            "name": "John and the sourcerers' stone",
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
#[mantra_macros::req_test("go.backup.import.relations")]
fn backup_self_ref_import_self_ref_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                        boss: User @primary @relation(name: "boss_minion")
                        minion: User @relation(name: "boss_minion")
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                        boss: User @primary @relation(name: "boss_minion")
                        minion: User @relation(name: "boss_minion")
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 31 })),
                common: Common { node: Some("db0".into()), capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "_bossID": m::r("doc0_0") }),
                expected_data: json!({}),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ExportBackup {
                format: "".into(),
                pretty: false,
                collections: vec![],
                missing_file: false,
                expected_data: json!({
                    "content": m::concat([json!(r#"{"User":[{"_bossID":""#), m::r("doc0_0"), json!(r#"","_docID":""#), m::r("doc0_0"), json!(r#"","_docIDNew":""#), m::r("doc0_0"), json!(r#"","age":31,"name":"Bob"}]}"#)]),
                }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ImportBackup {
                import_content: Text::concat([Text::from(r#"{"User":[{"_bossID":""#), Text::reference("doc0_0"), Text::from(r#"","_docID":""#), Text::reference("doc0_0"), Text::from(r#"","_docIDNew":""#), Text::reference("doc0_0"), Text::from(r#"","age":31,"name":"Bob"}]}"#)]),
                missing_file: false,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                        User {
                            name
                            boss {
                                name
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({ "User": [{ "boss": { "name": "Bob" }, "name": "Bob" }] }),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.backup.import.relations")]
fn backup_self_ref_import_simple_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                        boss: User @primary @relation(name: "boss_minion")
                        minion: User @relation(name: "boss_minion")
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            ImportBackup {
                import_content: r#"{
					"User":[
						{
							"_docID":"bae-692a9178-a258-5224-990f-9ad703a2bbea",
							"age":31,
							"_bossID":"bae-1635f80b-612a-5378-a185-cad7a3018354",
							"name":"Bob"
						},
						{
							"_docID":"bae-1635f80b-612a-5378-a185-cad7a3018354",
							"age":30,
							"name":"John"
						}
					]
				}"#
                .into(),
                missing_file: false,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                        User {
                            name
                            boss {
                                name
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "User": m::unordered(json!([
                        { "boss": null, "name": "John" },
                        { "boss": { "name": "John" }, "name": "Bob" },
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

#[test]
#[mantra_macros::req_test("go.backup.import.relations")]
fn backup_self_ref_import_split_primary_relation_with_second_collection_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        book: Book @primary @relation(name: "author_book")
                        reviewed: Book @relation(name: "reviewedBy_reviewed")
                    }
                    type Book {
                        name: String
                        author: Author @relation(name: "author_book")
                        reviewedBy: Author @primary @relation(name: "reviewedBy_reviewed")
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        book: Book @primary @relation(name: "author_book")
                        reviewed: Book @relation(name: "reviewedBy_reviewed")
                    }
                    type Book {
                        name: String
                        author: Author @relation(name: "author_book")
                        reviewedBy: Author @primary @relation(name: "reviewedBy_reviewed")
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "John and the sourcerers' stone" })),
                common: Common { node: Some("db0".into()), capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John", "book": "bae-89136f56-3779-5656-b8a6-f76a1c262f37" })),
                common: Common { node: Some("db0".into()), capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Book".into(),
                doc_id: Text::reference("doc1_0"),
                doc_map: json!({ "_reviewedByID": "bae-bf1f16db-3c02-5759-8127-7d73346442cc" }),
                expected_data: json!({}),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ImportBackup {
                import_content: r#"{"Author":[{"_docID":"bae-bf1f16db-3c02-5759-8127-7d73346442cc","_docIDNew":"bae-bf1f16db-3c02-5759-8127-7d73346442cc","_bookID":"bae-89136f56-3779-5656-b8a6-f76a1c262f37","name":"John"}],"Book":[{"_docID":"bae-89136f56-3779-5656-b8a6-f76a1c262f37","_docIDNew":"bae-66b0f769-c743-5a50-ae6d-1dcd978e2404","name":"John and the sourcerers' stone","_reviewedByID":"bae-bf1f16db-3c02-5759-8127-7d73346442cc"}]}"#.into(),
                missing_file: false,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Book {
                            name
                            author {
                                name
                                reviewed {
                                    name
                                }
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "Book": [
                        {
                            "author": {
                                "name": "John",
                                "reviewed": { "name": "John and the sourcerers' stone" },
                            },
                            "name": "John and the sourcerers' stone",
                        },
                    ],
                }),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
