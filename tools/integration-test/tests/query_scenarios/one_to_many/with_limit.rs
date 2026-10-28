//! Generated from the conformance scenarios of tests/integration/query/one_to_many/with_limit_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_many.child-order-paging")]
fn query_one_to_many_with_multiple_child_limits() {
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
                                            p1: published (limit: 1) {
                                                name
                                                rating
                                            }
                                            p2: published (limit: 2) {
                                                name
                                                rating
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John Grisham",
                            "p1": [{ "name": "Painted House", "rating": 4.9 }],
                            "p2": [
                                { "name": "Painted House", "rating": 4.9 },
                                { "name": "A Time for Mercy", "rating": 4.5 },
                            ],
                        },
                        {
                            "name": "Cornelia Funke",
                            "p1": [{ "name": "Theif Lord", "rating": 4.8 }],
                            "p2": [{ "name": "Theif Lord", "rating": 4.8 }],
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
#[mantra_macros::req_test("go.query.one_to_many.child-order-paging")]
fn query_one_to_many_with_single_child_limit() {
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
                                            published (limit: 1) {
                                                name
                                                rating
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John Grisham",
                            "published": [{ "name": "Painted House", "rating": 4.9 }],
                        },
                        {
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
