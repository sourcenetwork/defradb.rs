//! Generated from the conformance scenarios of tests/integration/query/one_to_many/with_group_related_id_alias_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_many.group.by-relation")]
fn query_one_to_many_with_parent_group_by_on_related_type_from_many_side_using_alias_and_related_selection(
) {
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
                                        Book(groupBy: [author]) {
                                            author {
                                                _docID
                                                name
                                            }
                                            GROUP {
                                                name
                                                rating
                                                author {
                                                    name
                                                    age
                                                }
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Book": [
                        {
                            "GROUP": [
                                {
                                    "author": { "age": 327, "name": "Voltaire" },
                                    "name": "Candide",
                                    "rating": 4.95,
                                },
                                {
                                    "author": { "age": 327, "name": "Voltaire" },
                                    "name": "Zadig",
                                    "rating": 4.91,
                                },
                            ],
                            "author": { "_docID": m::r("doc1_1"), "name": "Voltaire" },
                        },
                        {
                            "GROUP": [
                                {
                                    "author": { "age": 65, "name": "John Grisham" },
                                    "name": "Painted House",
                                    "rating": 4.9,
                                },
                                {
                                    "author": { "age": 65, "name": "John Grisham" },
                                    "name": "A Time for Mercy",
                                    "rating": 4.5,
                                },
                                {
                                    "author": { "age": 65, "name": "John Grisham" },
                                    "name": "The Client",
                                    "rating": 4.5,
                                },
                            ],
                            "author": { "_docID": m::r("doc1_0"), "name": "John Grisham" },
                        },
                        {
                            "GROUP": [
                                {
                                    "author": { "age": 327, "name": "Simon Pelloutier" },
                                    "name": "Histoiare des Celtes et particulierement des Gaulois et des Germains depuis les temps fabuleux jusqua la prise de Roze par les Gaulois",
                                    "rating": 2.0,
                                },
                            ],
                            "author": { "_docID": m::r("doc1_2"), "name": "Simon Pelloutier" },
                        },
                    ],
                }),
                common: Common { unordered_documents: vec!["/data/Book".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many.group.by-relation")]
fn query_one_to_many_with_parent_group_by_on_related_type_from_many_side_using_alias() {
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
                                        Book(groupBy: [author]) {
                                            _authorID
                                            GROUP {
                                                name
                                                rating
                                                author {
                                                    name
                                                    age
                                                }
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Book": [
                        {
                            "GROUP": [
                                {
                                    "author": { "age": 327, "name": "Voltaire" },
                                    "name": "Candide",
                                    "rating": 4.95,
                                },
                                {
                                    "author": { "age": 327, "name": "Voltaire" },
                                    "name": "Zadig",
                                    "rating": 4.91,
                                },
                            ],
                            "_authorID": m::r("doc1_1"),
                        },
                        {
                            "GROUP": [
                                {
                                    "author": { "age": 65, "name": "John Grisham" },
                                    "name": "Painted House",
                                    "rating": 4.9,
                                },
                                {
                                    "author": { "age": 65, "name": "John Grisham" },
                                    "name": "A Time for Mercy",
                                    "rating": 4.5,
                                },
                                {
                                    "author": { "age": 65, "name": "John Grisham" },
                                    "name": "The Client",
                                    "rating": 4.5,
                                },
                            ],
                            "_authorID": m::r("doc1_0"),
                        },
                        {
                            "GROUP": [
                                {
                                    "author": { "age": 327, "name": "Simon Pelloutier" },
                                    "name": "Histoiare des Celtes et particulierement des Gaulois et des Germains depuis les temps fabuleux jusqua la prise de Roze par les Gaulois",
                                    "rating": 2.0,
                                },
                            ],
                            "_authorID": m::r("doc1_2"),
                        },
                    ],
                }),
                common: Common { unordered_documents: vec!["/data/Book".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many.group.errors")]
fn query_one_to_many_with_parent_group_by_on_related_type_from_single_side_using_alias() {
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
                                        Author(groupBy: [published]) {
                                            GROUP {
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                expected_error: Some("cannot group by array or object field. Field: published".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many.group.by-relation")]
fn query_one_to_many_with_parent_group_by_on_related_type_with_id_selection_from_many_side_using_alias_and_related_selection(
) {
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
                                        Book(groupBy: [author]) {
                                            _authorID
                                            author {
                                                _docID
                                                name
                                            }
                                            GROUP {
                                                name
                                                rating
                                                author {
                                                    name
                                                    age
                                                }
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Book": [
                        {
                            "GROUP": [
                                {
                                    "author": { "age": 327, "name": "Voltaire" },
                                    "name": "Candide",
                                    "rating": 4.95,
                                },
                                {
                                    "author": { "age": 327, "name": "Voltaire" },
                                    "name": "Zadig",
                                    "rating": 4.91,
                                },
                            ],
                            "_authorID": m::r("doc1_1"),
                            "author": { "_docID": m::r("doc1_1"), "name": "Voltaire" },
                        },
                        {
                            "GROUP": [
                                {
                                    "author": { "age": 65, "name": "John Grisham" },
                                    "name": "Painted House",
                                    "rating": 4.9,
                                },
                                {
                                    "author": { "age": 65, "name": "John Grisham" },
                                    "name": "A Time for Mercy",
                                    "rating": 4.5,
                                },
                                {
                                    "author": { "age": 65, "name": "John Grisham" },
                                    "name": "The Client",
                                    "rating": 4.5,
                                },
                            ],
                            "_authorID": m::r("doc1_0"),
                            "author": { "_docID": m::r("doc1_0"), "name": "John Grisham" },
                        },
                        {
                            "GROUP": [
                                {
                                    "author": { "age": 327, "name": "Simon Pelloutier" },
                                    "name": "Histoiare des Celtes et particulierement des Gaulois et des Germains depuis les temps fabuleux jusqua la prise de Roze par les Gaulois",
                                    "rating": 2.0,
                                },
                            ],
                            "_authorID": m::r("doc1_2"),
                            "author": { "_docID": m::r("doc1_2"), "name": "Simon Pelloutier" },
                        },
                    ],
                }),
                common: Common { unordered_documents: vec!["/data/Book".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many.group.by-relation")]
fn query_one_to_many_with_parent_group_by_on_related_type_with_id_selection_from_many_side_using_alias(
) {
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
                                        Book(groupBy: [author]) {
                                            _authorID
                                            GROUP {
                                                name
                                                rating
                                                author {
                                                    name
                                                    age
                                                }
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Book": [
                        {
                            "GROUP": [
                                {
                                    "author": { "age": 327, "name": "Voltaire" },
                                    "name": "Candide",
                                    "rating": 4.95,
                                },
                                {
                                    "author": { "age": 327, "name": "Voltaire" },
                                    "name": "Zadig",
                                    "rating": 4.91,
                                },
                            ],
                            "_authorID": m::r("doc1_1"),
                        },
                        {
                            "GROUP": [
                                {
                                    "author": { "age": 65, "name": "John Grisham" },
                                    "name": "Painted House",
                                    "rating": 4.9,
                                },
                                {
                                    "author": { "age": 65, "name": "John Grisham" },
                                    "name": "A Time for Mercy",
                                    "rating": 4.5,
                                },
                                {
                                    "author": { "age": 65, "name": "John Grisham" },
                                    "name": "The Client",
                                    "rating": 4.5,
                                },
                            ],
                            "_authorID": m::r("doc1_0"),
                        },
                        {
                            "GROUP": [
                                {
                                    "author": { "age": 327, "name": "Simon Pelloutier" },
                                    "name": "Histoiare des Celtes et particulierement des Gaulois et des Germains depuis les temps fabuleux jusqua la prise de Roze par les Gaulois",
                                    "rating": 2.0,
                                },
                            ],
                            "_authorID": m::r("doc1_2"),
                        },
                    ],
                }),
                common: Common { unordered_documents: vec!["/data/Book".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_many.group.errors")]
fn query_one_to_many_with_parent_group_by_on_related_type_with_id_selection_from_single_side_using_alias(
) {
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
                                        Author(groupBy: [published]) {
                                            _publishedID
                                            GROUP {
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                expected_error: Some(r#"Cannot query field "_publishedID" on type "Author". "#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
