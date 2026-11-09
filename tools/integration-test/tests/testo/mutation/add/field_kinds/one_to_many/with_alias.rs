//! Generated from the conformance scenarios of tests/integration/mutation/add/field_kinds/one_to_many/with_alias_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Close, NewNode, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.relation.one_to_many_link")]
fn mutation_update_one_to_many_alias_relation_name_and_internal_id_both_produce_same_doc_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                common: Common { node: Some("case0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
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
                        published: [Book]
                    }
                "#.into(),
                common: Common { node: Some("case0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham" })),
                common: Common { node: Some("case0".into()), capture: captures([("doc1_0case0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc1_0case0"), "name": "Painted House" })),
                common: Common { node: Some("case0".into()), capture: captures([("doc0_0case0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Book { _docID } }".into(),
                expected_data: json!({ "Book": [{ "_docID": "bae-6dfd1924-d27b-55d0-9eaf-152b5d873fc1" }] }),
                common: Common { node: Some("case0".into()), compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Close {
                common: Common { node: Some("case0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                common: Common { node: Some("case1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
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
                        published: [Book]
                    }
                "#.into(),
                common: Common { node: Some("case1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham" })),
                common: Common { node: Some("case1".into()), capture: captures([("doc1_0case1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc1_0case1"), "name": "Painted House" })),
                common: Common { node: Some("case1".into()), capture: captures([("doc0_0case1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Book { _docID } }".into(),
                expected_data: json!({ "Book": [{ "_docID": "bae-6dfd1924-d27b-55d0-9eaf-152b5d873fc1" }] }),
                common: Common { node: Some("case1".into()), compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Close {
                common: Common { node: Some("case1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.relation.dangling")]
fn mutation_add_one_to_many_aliased_relation_name_non_existing_relation_many_side_added_doc() {
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
                        published: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "name": "Painted House",
                    "author": "bae-8627532a-2ed3-50ed-91d5-26f6b9b44c25",
                })),
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
                                        Book {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Book": [{ "name": "Painted House" }] }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.mutation.unknown_field")]
fn mutation_add_one_to_many_aliased_relation_name_non_existing_relation_single_side_no_id_field_error(
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
                        published: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "name": "John Grisham",
                    "published": "bae-8627532a-2ed3-50ed-91d5-26f6b9b44c25",
                })),
                expected_error: Some("the given field does not exist. Name: published".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.relation.one_to_many_link")]
fn mutation_add_one_to_many_aliased_relation_name_to_link_from_many_side() {
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
                        published: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc1_0"), "name": "Painted House" })),
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
                                        Author {
                                            name
                                            published {
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John Grisham",
                            "published": [{ "name": "Painted House" }],
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [{ "author": { "name": "John Grisham" }, "name": "Painted House" }],
                }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.mutation.unknown_field")]
fn mutation_add_one_to_many_aliased_relation_name_with_invalid_field_error() {
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
                        published: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "notName": "Painted House",
                    "author": "bae-8627532a-2ed3-50ed-91d5-26f6b9b44c25",
                })),
                expected_error: Some("the given field does not exist. Name: notName".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
