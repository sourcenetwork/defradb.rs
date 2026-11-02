//! Generated from the conformance scenarios of tests/integration/query/one_to_many/with_group_filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_many.group.join")]
fn query_one_to_many_with_group_by_and_filter_on_parent_relation() {
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
                doc_map: Some(json!({ "name": "Voltaire", "age": 327, "verified": true })),
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
                doc_map: Some(json!({ "name": "Candide", "rating": 4.95, "_authorID": m::r("doc1_1") })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Zadig", "rating": 4.91, "_authorID": m::r("doc1_1") })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Book (filter: {author: {name: {_like: "John%"}}}, groupBy: [rating]) {
                                                rating
                                                GROUP {
                                                    name
                                                    author { name }
                                                }
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "Book": [
                        {
                            "GROUP": [
                                {
                                    "author": { "name": "John Grisham" },
                                    "name": "A Time for Mercy",
                                },
                            ],
                            "rating": 4.5,
                        },
                        {
                            "GROUP": [
                                {
                                    "author": { "name": "John Grisham" },
                                    "name": "Painted House",
                                },
                            ],
                            "rating": 4.9,
                        },
                    ],
                }),
                common: Common { compare: Some(true), unordered_documents: vec!["/data/Book".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many.group.join")]
fn query_one_to_many_with_parent_join_group_number_and_number_filter_on_group_and_on_group_join() {
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
                doc_map: Some(json!({ "name": "The Client", "rating": 4.5, "_authorID": m::r("doc1_0") })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Candide", "rating": 4.95, "_authorID": m::r("doc1_1") })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Zadig", "rating": 4.91, "_authorID": m::r("doc1_1") })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "name": "Histoiare des Celtes et particulierement des Gaulois et des Germains depuis les temps fabuleux jusqua la prise de Roze par les Gaulois",
                    "rating": 2,
                    "_authorID": m::r("doc1_2"),
                })),
                common: Common { capture: captures([("doc0_5", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Author (groupBy: [age], filter: {age: {_gt: 300}}) {
                                                age
                                                GROUP {
                                                    name
                                                    published (filter: {rating: {_gt: 4.91}}){
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
                                { "name": "Simon Pelloutier", "published": [] },
                                {
                                    "name": "Voltaire",
                                    "published": [{ "name": "Candide", "rating": 4.95 }],
                                },
                            ],
                            "age": 327,
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
#[mantra_macros::req_test("go.query.one_to_many.group.join")]
fn query_one_to_many_with_parent_join_group_number_and_number_filter_on_group() {
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
#[mantra_macros::req_test("go.query.one_to_many.group.join")]
fn query_one_to_many_with_parent_join_group_number_and_number_filter_on_join() {
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
                doc_map: Some(json!({ "name": "The Client", "rating": 4.5, "_authorID": m::r("doc1_0") })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Candide", "rating": 4.95, "_authorID": m::r("doc1_1") })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Zadig", "rating": 4.91, "_authorID": m::r("doc1_1") })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "name": "Histoiare des Celtes et particulierement des Gaulois et des Germains depuis les temps fabuleux jusqua la prise de Roze par les Gaulois",
                    "rating": 2,
                    "_authorID": m::r("doc1_2"),
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
                                                GROUP {
                                                    name
                                                    published (filter: {rating: {_gt: 4.6}}) {
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
                                { "name": "Simon Pelloutier", "published": [] },
                            ],
                            "age": 327,
                        },
                        {
                            "GROUP": [
                                {
                                    "name": "John Grisham",
                                    "published": [{ "name": "Painted House", "rating": 4.9 }],
                                },
                            ],
                            "age": 65,
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
