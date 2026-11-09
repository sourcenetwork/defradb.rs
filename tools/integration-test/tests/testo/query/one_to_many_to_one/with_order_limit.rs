//! Generated from the conformance scenarios of tests/integration/query/one_to_many_to_one/with_order_limit_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.order")]
fn one_to_many_to_one_deep_order_by_sub_type_of_both_desc_and_asc() {
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
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "age": 65, "verified": true })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke", "age": 62, "verified": false })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Not a Writer", "age": 6, "verified": false })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_1"), "name": "The Rooster Bar", "rating": 4 })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "Theif Lord", "rating": 4.8 })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "The Associate", "rating": 4.2 })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "Painted House", "rating": 4.9 })),
                common: Common { capture: captures([("doc1_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "A Time for Mercy", "rating": 4.5 })),
                common: Common { capture: captures([("doc1_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "Sooley", "rating": 3.2 })),
                common: Common { capture: captures([("doc1_5", "/docID")]), ..Default::default() },
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
                common: Common { capture: captures([("doc2_0", "/docID")]), ..Default::default() },
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
                common: Common { capture: captures([("doc2_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "_bookID": m::r("doc1_3"),
                    "address": "600 Madison Ave., New York, New York",
                    "name": "Only Publisher of Painted House",
                    "yearOpened": 1995,
                })),
                common: Common { capture: captures([("doc2_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "_bookID": m::r("doc1_4"),
                    "address": "123 Andrew Street, Flin Flon, Manitoba",
                    "name": "Only Publisher of A Time for Mercy",
                    "yearOpened": 2013,
                })),
                common: Common { capture: captures([("doc2_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "_bookID": m::r("doc1_5"),
                    "address": "11 Sooley Ave., Waterloo, Ontario",
                    "name": "Only Publisher of Sooley",
                    "yearOpened": 1999,
                })),
                common: Common { capture: captures([("doc2_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Author {
                                            name
                                            NewestPublishersBook: book(order: {publisher: {yearOpened: DESC}}, limit: 1) {
                                                name
                                            }
                                            OldestPublishersBook: book(order: {publisher: {yearOpened: ASC}}, limit: 1) {
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Author": [
                        {
                            "NewestPublishersBook": [{ "name": "The Rooster Bar" }],
                            "OldestPublishersBook": [{ "name": "The Rooster Bar" }],
                            "name": "Cornelia Funke",
                        },
                        {
                            "NewestPublishersBook": [{ "name": "Theif Lord" }],
                            "OldestPublishersBook": [{ "name": "The Associate" }],
                            "name": "John Grisham",
                        },
                        {
                            "NewestPublishersBook": [],
                            "OldestPublishersBook": [],
                            "name": "Not a Writer",
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
