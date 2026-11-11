//! Generated from the conformance scenarios of tests/integration/backup/one_to_many/import_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, ImportBackup, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.backup.import.relations")]
fn backup_import_with_multiple_no_key_and_multiple_collections_and_updated_docs_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                        books: [Book]
                    }

                    type Book {
                        name: String
                        author: User
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            ImportBackup {
                import_content: r#"{
					"Book":[
						{
							"_docID":"bae-9828df35-b4cd-5d3a-acab-193c84e521c6",
							"_docIDNew":"bae-d7b5bc04-26af-570f-9aec-b9c5d923842f",
							"_authorID":"bae-1552bcf5-6b3b-5cd0-bdaf-33bb43f74ab4",
							"name":"Game of chains"
						},
						{
							"_docID":"bae-0fa157eb-c762-51af-859d-9d0eb941d2f4",
							"_docIDNew":"bae-8507cb9a-54ea-5db3-bb38-6b4e6e8f3dbf",
							"_authorID":"bae-1552bcf5-6b3b-5cd0-bdaf-33bb43f74ab4",
							"name":"John and the sourcerers' stone"
						}
					],
					"User":[
						{
							"_docID":"bae-be327e0b-a7fa-53ce-b29a-919cce5b5120",
							"_docIDNew":"bae-be327e0b-a7fa-53ce-b29a-919cce5b5120",
							"age":31,
							"name":"Bob"
						},
						{
							"_docID":"bae-3fc941b7-505c-5ce2-91a0-b180930ec8a9",
							"_docIDNew":"bae-1552bcf5-6b3b-5cd0-bdaf-33bb43f74ab4",
							"age":31,
							"name":"John"
						}
					]
				}"#.into(),
                missing_file: false,
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                        User {
                            name
                            age
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "User": m::unordered(json!([{ "age": 31, "name": "Bob" }, { "age": 31, "name": "John" }])),
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                        Book {
                            name
                            author {
                                _docID
                                name
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "Book": m::unordered(json!([
                        {
                            "author": { "_docID": m::doc_id(), "name": "John" },
                            "name": "John and the sourcerers' stone",
                        },
                        {
                            "author": { "_docID": m::doc_id(), "name": "John" },
                            "name": "Game of chains",
                        },
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
#[mantra_macros::req_test("go.backup.import.documents")]
fn backup_import_with_multiple_no_key_and_multiple_collections_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                        books: [Book]
                    }

                    type Book {
                        name: String
                        author: User
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            ImportBackup {
                import_content: r#"{
					"User":[
						{"age":30,"name":"John"},
						{"age":31,"name":"Smith"},
						{"age":32,"name":"Bob"}
					],
					"Book":[
						{"name":"John and the sourcerers' stone"},
						{"name":"Game of chains"}
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
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "User": m::unordered(json!([
                        { "age": 31, "name": "Smith" },
                        { "age": 30, "name": "John" },
                        { "age": 32, "name": "Bob" },
                    ])),
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                        Book {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": m::unordered(json!([
                        { "name": "Game of chains" },
                        { "name": "John and the sourcerers' stone" },
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
