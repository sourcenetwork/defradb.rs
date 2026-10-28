//! Generated from the conformance scenarios of tests/integration/query/one_to_one/with_filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.one_to_one.filter")]
fn query_one_to_one_with_boolean_filter_on_child_with_no_sub_type_selection() {
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
            Request {
                request: r#"
                    query {
                                        Book(filter: {author: {verified: {_eq: true}}}) {
                                            name
                                            rating
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Book": [{ "name": "Painted House", "rating": 4.9 }] }),
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
#[mantra_macros::req_test("go.query.one_to_one.filter")]
fn query_one_to_one_with_boolean_filter_on_child() {
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
                                        Book(filter: {author: {verified: {_eq: true}}}) {
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
#[mantra_macros::req_test("go.query.one_to_one.filter")]
fn query_one_to_one_with_compound_and_filter_that_includes_relation() {
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
                doc_map: Some(json!({ "name": "Some Book", "rating": 4.0 })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Some Other Book", "rating": 3.0 })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
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
                    "age": 45,
                    "name": "Some Writer",
                    "verified": false,
                })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "_publishedID": m::r("doc0_2"),
                    "age": 30,
                    "name": "Some Other Writer",
                    "verified": true,
                })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book(filter: {_and: [
                                            {rating: {_geq: 4.0}},
                                            {author: {verified: {_eq: true}}}
                                        ]}) {
                                            name
                                            rating
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Book": [{ "name": "Painted House", "rating": 4.9 }] }),
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
#[mantra_macros::req_test("go.query.one_to_one.filter")]
fn query_one_to_one_with_compound_filters_that_includes_relation_should_return_results() {
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
                doc_map: Some(json!({ "name": "Some Book", "rating": 4.0 })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Some Other Book", "rating": 3.0 })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
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
                    "age": 45,
                    "name": "Some Writer",
                    "verified": false,
                })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "_publishedID": m::r("doc0_2"),
                    "age": 30,
                    "name": "Some Other Writer",
                    "verified": true,
                })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book(filter: {_or: [
                                            {rating: {_gt: 4.0}},
                                            {author: {age: {_eq: 30}}}
                                        ]}) {
                                            name
                                            rating
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        { "name": "Painted House", "rating": 4.9 },
                        { "name": "Some Other Book", "rating": 3.0 },
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
            Request {
                request: r#"
                    query {
                                        Book(filter: {_and: [
                                            {rating: {_geq: 4.0}},
                                            {author: {age: {_eq: 45}}}
                                        ]}) {
                                            name
                                            rating
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Book": [{ "name": "Some Book", "rating": 4.0 }] }),
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
                                        Book(filter: {_not: {
                                            rating: {_geq: 4.0},
                                            author: {age: {_eq: 45}}
                                        }}) {
                                            name
                                            rating
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        { "name": "Painted House", "rating": 4.9 },
                        { "name": "Some Other Book", "rating": 3.0 },
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
#[mantra_macros::req_test("go.query.one_to_one.filter")]
fn query_one_to_one_with_compound_or_filter_that_includes_relation() {
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
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Painted House", "rating": 4.9 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Some Book", "rating": 4.0 })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Some Other Book", "rating": 3.5 })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Yet Another Book", "rating": 3.0 })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
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
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "_publishedID": m::r("doc0_1"),
                    "age": 45,
                    "name": "Some Writer",
                    "verified": false,
                })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "_publishedID": m::r("doc0_2"),
                    "age": 35,
                    "name": "Some Other Writer",
                    "verified": false,
                })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({
                    "_publishedID": m::r("doc0_3"),
                    "age": 30,
                    "name": "Yet Another Writer",
                    "verified": false,
                })),
                common: Common { capture: captures([("doc1_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book(filter: {_or: [
                                            {_and: [
                                                {rating: {_geq: 4.0}},
                                                {author: {age: {_leq: 45}}}
                                            ]},
                                            {_and: [
                                                {rating: {_leq: 3.5}},
                                                {author: {age: {_geq: 35}}}
                                            ]}
                                        ]}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Book": [{ "name": "Some Other Book" }, { "name": "Some Book" }] }),
                common: Common { compare: Some(true), unordered_documents: vec!["/data/Book".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book(filter: {_or: [
                                            {_not: {author: {age: {_lt: 65}}} },
                                            {_not: {author: {age: {_gt: 30}}} }
                                        ]}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Book": [{ "name": "Yet Another Book" }, { "name": "Painted House" }] }),
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
#[mantra_macros::req_test("go.query.one_to_one.filter")]
fn query_one_to_one_with_filter_through_child_back_to_parent() {
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
                doc_map: Some(json!({ "name": "Theif Lord", "rating": 4.8 })),
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
                    "age": 62,
                    "name": "Cornelia Funke",
                    "verified": false,
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
                                        Book(filter: {author: {published: {rating: {_eq: 4.9}}}}) {
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
#[mantra_macros::req_test("go.query.one_to_one.filter")]
fn query_one_to_one_with_numeric_filter_on_parent() {
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
            Request {
                request: r#"
                    query {
                                        Book {
                                            name
                                            rating
                                            author(filter: {age: {_eq: 65}}) {
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
#[mantra_macros::req_test("go.query.one_to_one.filter")]
fn query_one_to_one_with_string_filter_on_child() {
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
                                        Book(filter: {name: {_eq: "Painted House"}}) {
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
