//! Generated from the conformance scenarios of tests/integration/collection/add_relation_with_collection_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, PatchCollection, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection.relations_to_existing")]
fn collection_version_updates_add_field_kind_foreign_object_with_patch_adding_one_to_many_relation_after_separate_add_collections_should_succeed(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Author {
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
							"Name": "author", "Kind": "Author", "RelationName": "author_book", "IsPrimary": true
						}},
						{ "op": "add", "path": "/Book/Fields/-", "value": {
							"Name": "_authorID", "Kind": 1, "RelationName": "author_book", "IsPrimary": true
						}},
						{ "op": "add", "path": "/Author/Fields/-", "value": {
							"Name": "books", "Kind": "[Book]", "RelationName": "author_book"
						}}
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc1_0"), "name": "Painted House" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc1_0"), "name": "A Time to Kill" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Author {
                                            name
                                            books {
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Author": m::unordered(json!([
                        {
                            "books": m::unordered(json!([{ "name": "Painted House" }, { "name": "A Time to Kill" }])),
                            "name": "John Grisham",
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
#[mantra_macros::req_test("go.collection.relations_to_existing")]
fn collection_version_updates_add_field_kind_foreign_object_with_add_collection_creating_one_to_many_relation_to_existing_collection_should_succeed(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        author: Author
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "name": "Painted House" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "name": "A Time for Mercy" })),
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
                                        Book {
                                            name
                                            _authorID
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
                            "_authorID": m::r("doc0_0"),
                            "author": { "name": "John Grisham" },
                            "name": "A Time for Mercy",
                        },
                        {
                            "_authorID": m::r("doc0_0"),
                            "author": { "name": "John Grisham" },
                            "name": "Painted House",
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Book".into()],
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
#[mantra_macros::req_test("go.collection.relations_to_existing")]
fn collection_version_updates_add_field_kind_foreign_object_with_add_collection_creating_one_to_many_relations_to_multiple_existing_collections_should_succeed(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Publisher {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        author: Author
                        publisher: Publisher
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "Penguin Books" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "author": m::r("doc0_0"),
                    "name": "Painted House",
                    "publisher": m::r("doc1_0"),
                })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book {
                                            name
                                            _authorID
                                            author {
                                                name
                                            }
                                            _publisherID
                                            publisher {
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "_authorID": m::r("doc0_0"),
                            "_publisherID": m::r("doc1_0"),
                            "author": { "name": "John Grisham" },
                            "name": "Painted House",
                            "publisher": { "name": "Penguin Books" },
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
#[mantra_macros::req_test("go.collection.relations_to_existing")]
fn collection_version_updates_add_field_kind_foreign_object_with_chained_one_to_many_relations_across_separate_collections_should_succeed(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Publisher {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        publisher: Publisher
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        author: Author
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "Penguin Books" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "publisher": m::r("doc0_0") })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc1_0"), "name": "Painted House" })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
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
                                                publisher {
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
                                "name": "John Grisham",
                                "publisher": { "name": "Penguin Books" },
                            },
                            "name": "Painted House",
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.collection.relations_to_existing")]
fn collection_version_updates_add_field_kind_foreign_object_with_mixed_batch_having_relation_to_existing_and_new_collections_should_succeed(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Publisher {
                        name: String
                    }
                    type Book {
                        name: String
                        author: Author
                        publisher: Publisher
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "Penguin Books" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "author": m::r("doc0_0"),
                    "name": "Painted House",
                    "publisher": m::r("doc1_0"),
                })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
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
                                            }
                                            publisher {
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "author": { "name": "John Grisham" },
                            "name": "Painted House",
                            "publisher": { "name": "Penguin Books" },
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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
