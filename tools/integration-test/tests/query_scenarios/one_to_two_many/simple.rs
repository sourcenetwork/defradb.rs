//! Generated from the conformance scenarios of tests/integration/query/one_to_two_many/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_two_many.named")]
fn query_one_to_two_many_with_named_and_unnamed_relationships_from_many_side() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        price: Price
                        author: Author @relation(name: "written_books")
                        reviewedBy: Author @relation(name: "reviewed_books")
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        written: [Book] @relation(name: "written_books")
                        reviewed: [Book] @relation(name: "reviewed_books")
                    }

                    type Price {
                        currency: String
                        value: Float
                        books: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Price".into(),
                doc_map: Some(json!({ "currency": "GBP", "value": 12.99 })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Price".into(),
                doc_map: Some(json!({ "currency": "SEK", "value": 129 })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
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
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke", "age": 62, "verified": false })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_0"),
                    "_priceID": m::r("doc2_0"),
                    "_reviewedByID": m::r("doc1_1"),
                    "name": "Painted House",
                    "rating": 4.9,
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_0"),
                    "_priceID": m::r("doc2_1"),
                    "_reviewedByID": m::r("doc1_1"),
                    "name": "A Time for Mercy",
                    "rating": 4.5,
                })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_1"),
                    "_priceID": m::r("doc2_0"),
                    "_reviewedByID": m::r("doc1_0"),
                    "name": "Theif Lord",
                    "rating": 4.8,
                })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
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
                                            age
                                            written {
                                                name
                                                price {
                                                    value
                                                }
                                            }
                                            reviewed {
                                                name
                                                rating
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "age": 65,
                            "name": "John Grisham",
                            "reviewed": [{ "name": "Theif Lord", "rating": 4.8 }],
                            "written": [
                                {
                                    "name": "A Time for Mercy",
                                    "price": { "value": 129.0 },
                                },
                                { "name": "Painted House", "price": { "value": 12.99 } },
                            ],
                        },
                        {
                            "age": 62,
                            "name": "Cornelia Funke",
                            "reviewed": [
                                { "name": "A Time for Mercy", "rating": 4.5 },
                                { "name": "Painted House", "rating": 4.9 },
                            ],
                            "written": [{ "name": "Theif Lord", "price": { "value": 12.99 } }],
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    unordered_documents: vec!["/data/Author".into()],
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
#[mantra_macros::req_test("go.query.one_to_two_many.named")]
fn query_one_to_two_many_with_named_and_unnamed_relationships() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        price: Price
                        author: Author @relation(name: "written_books")
                        reviewedBy: Author @relation(name: "reviewed_books")
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        written: [Book] @relation(name: "written_books")
                        reviewed: [Book] @relation(name: "reviewed_books")
                    }

                    type Price {
                        currency: String
                        value: Float
                        books: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Price".into(),
                doc_map: Some(json!({ "currency": "GBP", "value": 12.99 })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Price".into(),
                doc_map: Some(json!({ "currency": "SEK", "value": 129 })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
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
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke", "age": 62, "verified": false })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_0"),
                    "_priceID": m::r("doc2_0"),
                    "_reviewedByID": m::r("doc1_1"),
                    "name": "Painted House",
                    "rating": 4.9,
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_0"),
                    "_priceID": m::r("doc2_1"),
                    "_reviewedByID": m::r("doc1_1"),
                    "name": "A Time for Mercy",
                    "rating": 4.5,
                })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_1"),
                    "_priceID": m::r("doc2_0"),
                    "_reviewedByID": m::r("doc1_0"),
                    "name": "Theif Lord",
                    "rating": 4.8,
                })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
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
                                            }
                                            reviewedBy {
                                                name
                                                age
                                            }
                                            price {
                                                currency
                                                value
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "author": { "name": "John Grisham" },
                            "name": "A Time for Mercy",
                            "price": { "currency": "SEK", "value": 129.0 },
                            "rating": 4.5,
                            "reviewedBy": { "age": 62, "name": "Cornelia Funke" },
                        },
                        {
                            "author": { "name": "Cornelia Funke" },
                            "name": "Theif Lord",
                            "price": { "currency": "GBP", "value": 12.99 },
                            "rating": 4.8,
                            "reviewedBy": { "age": 65, "name": "John Grisham" },
                        },
                        {
                            "author": { "name": "John Grisham" },
                            "name": "Painted House",
                            "price": { "currency": "GBP", "value": 12.99 },
                            "rating": 4.9,
                            "reviewedBy": { "age": 62, "name": "Cornelia Funke" },
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.one_to_two_many.named")]
fn query_one_to_two_many_with_nil_unnamed_relationship_from_many_side() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author @relation(name: "written_books")
                        reviewedBy: Author @relation(name: "reviewed_books")
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        written: [Book] @relation(name: "written_books")
                        reviewed: [Book] @relation(name: "reviewed_books")
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
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke", "age": 62, "verified": false })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_0"),
                    "_reviewedByID": m::r("doc1_1"),
                    "name": "Painted House",
                    "rating": 4.9,
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_0"),
                    "_reviewedByID": m::r("doc1_1"),
                    "name": "A Time for Mercy",
                    "rating": 4.5,
                })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_1"),
                    "_reviewedByID": m::r("doc1_0"),
                    "name": "Theif Lord",
                    "rating": 4.8,
                })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
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
                                            age
                                            written {
                                                name
                                            }
                                            reviewed {
                                                name
                                                rating
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "age": 65,
                            "name": "John Grisham",
                            "reviewed": [{ "name": "Theif Lord", "rating": 4.8 }],
                            "written": [
                                { "name": "Painted House" },
                                { "name": "A Time for Mercy" },
                            ],
                        },
                        {
                            "age": 62,
                            "name": "Cornelia Funke",
                            "reviewed": [
                                { "name": "Painted House", "rating": 4.9 },
                                { "name": "A Time for Mercy", "rating": 4.5 },
                            ],
                            "written": [{ "name": "Theif Lord" }],
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    unordered_documents: vec!["/data/Author".into()],
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
#[mantra_macros::req_test("go.query.one_to_two_many.named")]
fn query_one_to_two_many_with_nil_unnamed_relationship_from_one_side() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author @relation(name: "written_books")
                        reviewedBy: Author @relation(name: "reviewed_books")
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        written: [Book] @relation(name: "written_books")
                        reviewed: [Book] @relation(name: "reviewed_books")
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
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke", "age": 62, "verified": false })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_0"),
                    "_reviewedByID": m::r("doc1_1"),
                    "name": "Painted House",
                    "rating": 4.9,
                })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_0"),
                    "_reviewedByID": m::r("doc1_1"),
                    "name": "A Time for Mercy",
                    "rating": 4.5,
                })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_1"),
                    "_reviewedByID": m::r("doc1_0"),
                    "name": "Theif Lord",
                    "rating": 4.8,
                })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
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
                                            }
                                            reviewedBy {
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
                            "author": { "name": "John Grisham" },
                            "name": "Painted House",
                            "rating": 4.9,
                            "reviewedBy": { "age": 62, "name": "Cornelia Funke" },
                        },
                        {
                            "author": { "name": "Cornelia Funke" },
                            "name": "Theif Lord",
                            "rating": 4.8,
                            "reviewedBy": { "age": 65, "name": "John Grisham" },
                        },
                        {
                            "author": { "name": "John Grisham" },
                            "name": "A Time for Mercy",
                            "rating": 4.5,
                            "reviewedBy": { "age": 62, "name": "Cornelia Funke" },
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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
