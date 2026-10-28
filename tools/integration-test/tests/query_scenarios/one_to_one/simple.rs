//! Generated from the conformance scenarios of tests/integration/query/one_to_one/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_one.join")]
fn query_one_to_one_primary_direction() {
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
                        published: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Painted House", "rating": 4.9 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "name": "John Grisham",
                    "age": 65,
                    "verified": true,
                    "_publishedID": m::r("doc0_0"),
                })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
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
                                                rating
                                                author {
                                                    name
                                                    age
                                                }
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "author": { "age": 65, "name": "John Grisham" },
                            "name": "Painted House",
                            "rating": 4.9,
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
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_one.join")]
fn query_one_to_one_secondary_direction() {
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
                        published: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Painted House", "rating": 4.9 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "name": "John Grisham",
                    "age": 65,
                    "verified": true,
                    "_publishedID": m::r("doc0_0"),
                })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
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
                                                age
                                                published {
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
                            "age": 65,
                            "name": "John Grisham",
                            "published": { "name": "Painted House", "rating": 4.9 },
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
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_one.join")]
fn query_one_to_one_with_multiple_records_secondary_direction() {
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
                        published: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Painted House" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Theif Lord" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "_publishedID": m::r("doc0_0"), "name": "John Grisham" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "_publishedID": m::r("doc0_1"), "name": "Cornelia Funke" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
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
                            "published": { "name": "Painted House" },
                        },
                        {
                            "name": "Cornelia Funke",
                            "published": { "name": "Theif Lord" },
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

#[test]
#[mantra_macros::req_test("go.query.one_to_one.join")]
fn query_one_to_one_with_multiple_records() {
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
                        published: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Painted House", "rating": 4.9 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Go Guide for Rust developers", "rating": 5.0 })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "_publishedID": m::r("doc0_0"),
                    "age": 65,
                    "name": "John Grisham",
                    "verified": true,
                })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "_publishedID": m::r("doc0_1"),
                    "age": 30,
                    "name": "Andrew Lone",
                    "verified": true,
                })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
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
                    "Book": [
                        { "author": { "name": "John Grisham" }, "name": "Painted House" },
                        {
                            "author": { "name": "Andrew Lone" },
                            "name": "Go Guide for Rust developers",
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
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_one.join")]
fn query_one_to_one_with_nil_child() {
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
                        published: Book @primary
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
                expected_data: json!({ "Author": [{ "name": "John Grisham", "published": null }] }),
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
#[mantra_macros::req_test("go.query.one_to_one.join")]
fn query_one_to_one_with_nil_parent() {
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
                        published: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Painted House" })),
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
                                            author {
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Book": [{ "author": null, "name": "Painted House" }] }),
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
#[mantra_macros::req_test("go.query.one_to_one.relation-id")]
fn query_one_to_one_with_relation_id_from_primary_side() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        author: Author
                    }

                    type Author {
                        name: String
                        published: Book @primary
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Painted House" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "_publishedID": m::r("doc0_0"), "name": "John Grisham" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Author {
                                            name
                                            _publishedID
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Author": [{ "_publishedID": m::r("doc0_0"), "name": "John Grisham" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.one_to_one.relation-id")]
fn query_one_to_one_with_relation_id_from_secondary_side() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        author: Author
                    }

                    type Author {
                        name: String
                        published: Book @primary
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Painted House" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "_publishedID": m::r("doc0_0"), "name": "John Grisham" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book {
                                            name
                                            _authorID
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Book": [{ "_authorID": m::r("doc1_0"), "name": "Painted House" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
