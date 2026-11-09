//! Generated from the conformance scenarios of tests/integration/index/query_with_group_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.relation.combined-filter")]
fn index_group_by_with_filter_on_indexed_relation_returns_group_by_field_value() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String 
                        rating: Float @index
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
                doc_map: Some(json!({ "name": "Voltaire", "age": 327, "verified": true })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Simon Pelloutier", "age": 327, "verified": true })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc1_0"), "name": "Painted House", "rating": 4.9 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc1_0"), "name": "A Time for Mercy", "rating": 4.5 })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc1_0"), "name": "The Client", "rating": 4.5 })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc1_1"), "name": "Candide", "rating": 4.95 })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc1_1"), "name": "Zadig", "rating": 4.91 })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_authorID": m::r("doc1_2"),
                    "name": "Histoiare des Celtes et particulierement des Gaulois et des Germains depuis les temps fabuleux jusqua la prise de Roze par les Gaulois",
                    "rating": 2,
                })),
                common: Common { capture: captures([("doc0_5", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Author (groupBy: [age]) {
                                                age
                                                GROUP (filter: {published: {rating: {_gt: 4.6}}}) {
                                                    name
                                                    published {
                                                        name
                                                        rating
                                                    }
                                                }
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "Author": [
                        {
                            "GROUP": [
                                {
                                    "name": "Voltaire",
                                    "published": [
                                        { "name": "Candide", "rating": 4.95 },
                                        { "name": "Zadig", "rating": 4.91 },
                                    ],
                                },
                            ],
                            "age": 327,
                        },
                        {
                            "GROUP": [
                                {
                                    "name": "John Grisham",
                                    "published": [
                                        { "name": "Painted House", "rating": 4.9 },
                                        { "name": "A Time for Mercy", "rating": 4.5 },
                                        { "name": "The Client", "rating": 4.5 },
                                    ],
                                },
                            ],
                            "age": 65,
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
