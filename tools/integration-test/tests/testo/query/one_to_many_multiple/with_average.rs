//! Generated from the conformance scenarios of tests/integration/query/one_to_many_multiple/with_average_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_many_multiple.avg")]
fn query_one_to_many_multiple_with_average_on_multiple_joins() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Article {
                        name: String
                        author: Author
                        rating: Int
                    }

                    type Book {
                        name: String
                        author: Author
                        score: Int
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        books: [Book]
                        articles: [Article]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "age": 65, "name": "John Grisham", "verified": true })),
                common: Common { capture: captures([("doc2_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "age": 62, "name": "Cornelia Funke", "verified": false })),
                common: Common { capture: captures([("doc2_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Article".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc2_0"),
                    "name": "After Guantánamo, Another Injustice",
                    "rating": 3,
                })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Article".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc2_1"), "name": "To my dear readers", "rating": 2 })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Article".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc2_1"),
                    "name": "Twinklestar's Favourite Xmas Cookie",
                    "rating": 1,
                })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc2_0"), "name": "Painted House", "score": 1 })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc2_0"), "name": "A Time for Mercy", "score": 2 })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc2_0"), "name": "Sooley", "score": 3 })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc2_1"), "name": "Theif Lord", "score": 4 })),
                common: Common { capture: captures([("doc1_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Author {
                                                name
                                                AVG(books: {field: score}, articles: {field: rating})
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "Author": [
                        { "AVG": 2.25, "name": "John Grisham" },
                        { "AVG": 2.3333333333333335, "name": "Cornelia Funke" },
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
