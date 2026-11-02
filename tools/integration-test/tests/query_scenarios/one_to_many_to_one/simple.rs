//! Generated from the conformance scenarios of tests/integration/query/one_to_many_to_one/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.joins")]
fn query_one_to_one_relations() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        favouritePageNumbers: [Int!]
                        book: [Book]
                    }

                    type Book {
                        name: String
                        rating: Float
                        author: Author
                        publisher: Publisher
                    }

                    type Publisher {
                        name: String
                        address: String
                        yearOpened: Int
                        book: Book @primary
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
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke", "age": 62, "verified": false })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Not a Writer", "age": 6, "verified": false })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "_authorID": m::r("doc0_1"), "name": "The Rooster Bar", "rating": 4 }),
                ),
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
                    json!({ "_authorID": m::r("doc0_0"), "name": "Theif Lord", "rating": 4.8 }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "_authorID": m::r("doc0_0"), "name": "The Associate", "rating": 4.2 }),
                ),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "_bookID": m::r("doc1_0"),
                    "address": "1 Rooster Ave., Waterloo, Ontario",
                    "name": "Only Publisher of The Rooster Bar",
                    "yearOpened": 2022,
                })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "_bookID": m::r("doc1_1"),
                    "address": "1 Theif Lord, Waterloo, Ontario",
                    "name": "Only Publisher of Theif Lord",
                    "yearOpened": 2020,
                })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
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
                            "name": "The Associate",
                            "publisher": null,
                        },
                        {
                            "author": { "name": "Cornelia Funke" },
                            "name": "The Rooster Bar",
                            "publisher": { "name": "Only Publisher of The Rooster Bar" },
                        },
                        {
                            "author": { "name": "John Grisham" },
                            "name": "Theif Lord",
                            "publisher": { "name": "Only Publisher of Theif Lord" },
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
