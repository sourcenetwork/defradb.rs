//! Generated from the conformance scenarios of tests/integration/query/one_to_many/with_group_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_many.group.join")]
fn query_one_to_many_with_duplicate_relation_selection_each_with_inner_group_by_on_relation() {
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
            Request {
                request: r#"
                    query @explain(type: debug) {
                                        Author {
                                            published(groupBy: [author]) {
                                                author {
                                                    name
                                                }
                                                GROUP {
                                                    name
                                                }
                                            }
                                            published(groupBy: [author]) {
                                                author {
                                                    name
                                                }
                                                GROUP {
                                                    name
                                                }
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "explain": {
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "selectNode": {
                                        "typeIndexJoin": {
                                            "typeJoinMany": {
                                                "root": { "scanNode": {} },
                                                "subType": {
                                                    "selectTopNode": {
                                                        "groupNode": {
                                                            "selectNode": {
                                                                "pipeNode": {
                                                                    "typeIndexJoin": {
                                                                        "typeJoinOne": {
                                                                            "root": {
                                                                                "scanNode": {},
                                                                            },
                                                                            "subType": {
                                                                                "selectTopNode": {
                                                                                    "selectNode": {
                                                                                        "scanNode": {},
                                                                                    },
                                                                                },
                                                                            },
                                                                        },
                                                                    },
                                                                },
                                                            },
                                                        },
                                                    },
                                                },
                                            },
                                        },
                                    },
                                },
                            },
                        ],
                    },
                }),
                common: Common { compare: Some(true), ..Default::default() },
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
fn query_one_to_many_with_group_by_genre_and_non_group_author_selected() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Person {
                        name: String!
                        authoredBooks: [Book]
                    }

                    type Company {
                        name: String!
                        sells: [Book]
                    }

                    type Book {
                        title: String!
                        genre: String
                        plot: String
                        rating: Float
                        author: Person
                        seller: Company
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book(groupBy: [genre]) {
                                            genre
                                            author {
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_error: Some("cannot select a non-group-by field at group-level".into()),
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
fn query_one_to_many_with_inner_join_group_number_with_non_group_fields_selected() {
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
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Author {
                                            name
                                            age
                                            published (groupBy: [rating]){
                                                rating
                                                name
                                                GROUP {
                                                    name
                                                }
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_error: Some("cannot select a non-group-by field at group-level".into()),
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
fn query_one_to_many_with_inner_join_group_number() {
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
                doc_map: Some(json!({ "name": "The Client", "rating": 4.5, "_authorID": m::r("doc1_0") })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Theif Lord", "rating": 4.8, "_authorID": m::r("doc1_1") })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Author {
                                                name
                                                age
                                                published (groupBy: [rating]){
                                                    rating
                                                    GROUP {
                                                        name
                                                    }
                                                }
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "Author": [
                        {
                            "age": 65,
                            "name": "John Grisham",
                            "published": [
                                { "GROUP": [{ "name": "Painted House" }], "rating": 4.9 },
                                {
                                    "GROUP": [
                                        { "name": "A Time for Mercy" },
                                        { "name": "The Client" },
                                    ],
                                    "rating": 4.5,
                                },
                            ],
                        },
                        {
                            "age": 62,
                            "name": "Cornelia Funke",
                            "published": [{ "GROUP": [{ "name": "Theif Lord" }], "rating": 4.8 }],
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
fn query_one_to_many_with_parent_group_by_on_relation_and_duplicate_relation_selection() {
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
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "name": "Painted House", "rating": 4.9, "_authorID": m::r("doc1_0") }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book(groupBy: [author]) {
                                            author {
                                                name
                                            }
                                            author {
                                                name
                                            }
                                            GROUP {
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "GROUP": [{ "name": "Painted House" }],
                            "author": { "name": "John Grisham" },
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
#[mantra_macros::req_test("go.query.one_to_many.group.join")]
fn query_one_to_many_with_parent_join_group_number() {
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
                                {
                                    "name": "Simon Pelloutier",
                                    "published": [
                                        {
                                            "name": "Histoiare des Celtes et particulierement des Gaulois et des Germains depuis les temps fabuleux jusqua la prise de Roze par les Gaulois",
                                            "rating": 2.0,
                                        },
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
