//! Generated from the conformance scenarios of tests/integration/query/one_to_one/with_group_related_id_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_one.group")]
fn query_one_to_one_with_group_related_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        author: Author @primary
                    }

                    type Author {
                        name: String
                        published: Book
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
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Andrew Lone" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc1_0"), "name": "Painted House" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "_authorID": m::r("doc1_1"), "name": "Go Guide for Rust developers" }),
                ),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book(groupBy: [_authorID]) {
                                            _authorID
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
                            "_authorID": m::r("doc1_0"),
                        },
                        {
                            "GROUP": [{ "name": "Go Guide for Rust developers" }],
                            "_authorID": m::r("doc1_1"),
                        },
                    ],
                }),
                common: Common {
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
#[mantra_macros::req_test("go.query.one_to_one.group")]
fn query_one_to_one_with_group_related_id_from_secondary_with_group_with_join() {
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
                doc_map: Some(json!({ "name": "Go Guide for Rust developers" })),
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
                doc_map: Some(json!({ "_publishedID": m::r("doc0_1"), "name": "Andrew Lone" })),
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
                                        Book(groupBy: [_authorID]) {
                                            _authorID
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
                            "_authorID": m::r("doc1_0"),
                            "author": { "name": "John Grisham" },
                        },
                        {
                            "GROUP": [{ "name": "Go Guide for Rust developers" }],
                            "_authorID": m::r("doc1_1"),
                            "author": { "name": "Andrew Lone" },
                        },
                    ],
                }),
                common: Common {
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
#[mantra_macros::req_test("go.query.one_to_one.group")]
fn query_one_to_one_with_group_related_id_from_secondary_with_group() {
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
                doc_map: Some(json!({ "name": "Go Guide for Rust developers" })),
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
                doc_map: Some(json!({ "_publishedID": m::r("doc0_1"), "name": "John Grisham" })),
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
                                        Book(groupBy: [_authorID]) {
                                            _authorID
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
                            "_authorID": m::r("doc1_0"),
                        },
                        {
                            "GROUP": [{ "name": "Go Guide for Rust developers" }],
                            "_authorID": m::r("doc1_1"),
                        },
                    ],
                }),
                common: Common {
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
#[mantra_macros::req_test("go.query.one_to_one.group")]
fn query_one_to_one_with_group_related_id_from_secondary_without_group_with_join() {
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
                doc_map: Some(json!({ "name": "Go Guide for Rust developers" })),
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
                doc_map: Some(json!({ "_publishedID": m::r("doc0_1"), "name": "Andrew Lone" })),
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
                                        Book(groupBy: [_authorID]) {
                                            _authorID
                                            author {
                                                name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {
                            "_authorID": m::r("doc1_0"),
                            "author": { "name": "John Grisham" },
                        },
                        {
                            "_authorID": m::r("doc1_1"),
                            "author": { "name": "Andrew Lone" },
                        },
                    ],
                }),
                common: Common {
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
#[mantra_macros::req_test("go.query.one_to_one.group")]
fn query_one_to_one_with_group_related_id_from_secondary_without_group() {
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
                doc_map: Some(json!({ "name": "Go Guide for Rust developers" })),
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
                doc_map: Some(json!({ "_publishedID": m::r("doc0_1"), "name": "Andrew Lone" })),
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
                                        Book(groupBy: [_authorID]) {
                                            _authorID
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [{ "_authorID": m::r("doc1_1") }, { "_authorID": m::r("doc1_0") }],
                }),
                common: Common {
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
