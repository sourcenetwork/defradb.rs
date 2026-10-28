//! Generated from the conformance scenarios of tests/integration/query/one_to_two_many/with_order_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_two_many.named")]
fn query_one_to_two_many_with_order() {
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
                    "_reviewedByID": m::r("doc1_0"),
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
                                            written (order: {rating: ASC}) {
                                                name
                                            }
                                            reviewed (order: {rating: DESC}){
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
                            "name": "Cornelia Funke",
                            "reviewed": [{ "name": "Painted House", "rating": 4.9 }],
                            "written": [{ "name": "Theif Lord" }],
                        },
                        {
                            "name": "John Grisham",
                            "reviewed": [
                                { "name": "Theif Lord", "rating": 4.8 },
                                { "name": "A Time for Mercy", "rating": 4.5 },
                            ],
                            "written": [
                                { "name": "A Time for Mercy" },
                                { "name": "Painted House" },
                            ],
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
