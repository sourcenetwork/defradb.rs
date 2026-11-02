//! Generated from the conformance scenarios of tests/integration/query/one_to_many_to_one/joins_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_many_to_one.joins")]
fn one_to_many_to_one_joins_are_linked_properly() {
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
                                            _docID
                                            name
                                            book {
                                                _docID
                                                name
                                                publisher {
                                                    _docID
                                                    name
                                                }
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Author": [
                        { "_docID": m::r("doc0_2"), "book": [], "name": "Not a Writer" },
                        {
                            "_docID": m::r("doc0_0"),
                            "book": [
                                {
                                    "_docID": m::r("doc1_2"),
                                    "name": "The Associate",
                                    "publisher": null,
                                },
                                {
                                    "_docID": m::r("doc1_5"),
                                    "name": "Sooley",
                                    "publisher": {
                                        "_docID": m::r("doc2_4"),
                                        "name": "Only Publisher of Sooley",
                                    },
                                },
                                {
                                    "_docID": m::r("doc1_3"),
                                    "name": "Painted House",
                                    "publisher": {
                                        "_docID": m::r("doc2_2"),
                                        "name": "Only Publisher of Painted House",
                                    },
                                },
                                {
                                    "_docID": m::r("doc1_4"),
                                    "name": "A Time for Mercy",
                                    "publisher": {
                                        "_docID": m::r("doc2_3"),
                                        "name": "Only Publisher of A Time for Mercy",
                                    },
                                },
                                {
                                    "_docID": m::r("doc1_1"),
                                    "name": "Theif Lord",
                                    "publisher": {
                                        "_docID": m::r("doc2_1"),
                                        "name": "Only Publisher of Theif Lord",
                                    },
                                },
                            ],
                            "name": "John Grisham",
                        },
                        {
                            "_docID": m::r("doc0_1"),
                            "book": [
                                {
                                    "_docID": m::r("doc1_0"),
                                    "name": "The Rooster Bar",
                                    "publisher": {
                                        "_docID": m::r("doc2_0"),
                                        "name": "Only Publisher of The Rooster Bar",
                                    },
                                },
                            ],
                            "name": "Cornelia Funke",
                        },
                    ],
                }),
                common: Common { unordered_documents: vec!["/data/Author".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
