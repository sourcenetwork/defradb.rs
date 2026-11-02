//! Generated from the conformance scenarios of tests/integration/query/one_to_many/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_many.join")]
fn query_one_to_many_primary_direction() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        published: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "age": 65, "verified": true })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "name": "Painted House", "rating": 4.9, "_authorID": m::r("doc1_0") }),
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
                                            Book {
                                                name
                                                rating
                                                author {
                                                    name
                                                    age
                                                }
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "author": { "age": 65, "name": "John Grisham" },
                            "name": "Painted House",
                            "rating": 4.9,
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
#[mantra_macros::req_test("go.query.one_to_many.join")]
fn query_one_to_many_secondary_direction() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        published: [Book]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "age": 65, "verified": true })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke", "age": 62, "verified": false })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Painted House", "rating": 4.9, "_authorID": m::r("doc1_0") })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "A Time for Mercy", "rating": 4.5, "_authorID": m::r("doc1_0") })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Theif Lord", "rating": 4.8, "_authorID": m::r("doc1_1") })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Author {
                                                name
                                                age
                                                published {
                                                    name
                                                    rating
                                                }
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "Author": [
                        {
                            "age": 65,
                            "name": "John Grisham",
                            "published": [
                                { "name": "Painted House", "rating": 4.9 },
                                { "name": "A Time for Mercy", "rating": 4.5 },
                            ],
                        },
                        {
                            "age": 62,
                            "name": "Cornelia Funke",
                            "published": [{ "name": "Theif Lord", "rating": 4.8 }],
                        },
                    ],
                }),
                common: Common { compare: Some(true), unordered_documents: vec!["/data/Author".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many.join")]
fn query_one_to_many_with_non_existant_parent() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        published: [Book]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "name": "Painted House",
                    "rating": 4.9,
                    "_authorID": "bae-8627532a-2ed3-50ed-91d5-26f6b9b44c25",
                })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book {
                                            name
                                            rating
                                            author {
                                                name
                                                age
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Book": [{ "author": null, "name": "Painted House", "rating": 4.9 }] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
