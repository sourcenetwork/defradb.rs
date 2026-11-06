//! Generated from the conformance scenarios of tests/integration/index/query_with_relation_sub_order_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-exhaustive")]
fn query_with_filter_on_null_relation_secondary_doc_without_relation_should_return_orphans() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        establishedYear: Int @index
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Book With Publisher" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan Book 1" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan Book 2" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc0_0"),
                    "establishedYear": 2020,
                    "name": "Publisher2020",
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
                    query @exhaustive {
                            Book(order: {publisher: {establishedYear: ASC}}) {
                                title
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        { "title": "Orphan Book 1" },
                        { "title": "Orphan Book 2" },
                        { "title": "Book With Publisher" },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-exhaustive")]
fn query_with_nested_order_by_relation_field_exhaustive_with_asc_and_limit_should_include_orphans_first(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        title: String
                        author: Author
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        establishedYear: Int @index
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2020" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2010" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2000" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "OrphanBook" })),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_0"),
                    "establishedYear": 2020,
                    "name": "Publisher2020",
                })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_1"),
                    "establishedYear": 2010,
                    "name": "Publisher2010",
                })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_2"),
                    "establishedYear": 2000,
                    "name": "Publisher2000",
                })),
                common: Common {
                    capture: captures([("doc2_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @exhaustive {
                            Author {
                                name
                                published(order: {publisher: {establishedYear: ASC}}, limit: 2) {
                                    title
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John",
                            "published": [{ "title": "OrphanBook" }, { "title": "Book2000" }],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-exhaustive")]
fn query_with_nested_order_by_relation_field_exhaustive_with_desc_and_limit_should_append_orphans_last(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        title: String
                        author: Author
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        establishedYear: Int @index
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2020" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2010" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2000" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "OrphanBook" })),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_0"),
                    "establishedYear": 2020,
                    "name": "Publisher2020",
                })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_1"),
                    "establishedYear": 2010,
                    "name": "Publisher2010",
                })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_2"),
                    "establishedYear": 2000,
                    "name": "Publisher2000",
                })),
                common: Common {
                    capture: captures([("doc2_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @exhaustive {
                            Author {
                                name
                                published(order: {publisher: {establishedYear: DESC}}, limit: 2) {
                                    title
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John",
                            "published": [{ "title": "Book2020" }, { "title": "Book2010" }],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-exhaustive")]
fn query_with_nested_order_by_relation_field_exhaustive_with_primary_parent_asc_should_include_orphans(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        book: [Book]
                    }
                    type Book {
                        title: String
                        author: Author
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        yearOpened: Int @index
                        book: [Book]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "Publisher2020", "yearOpened": 2020 })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "_publisherID": m::r("doc2_0"),
                    "author": m::r("doc0_0"),
                    "title": "LinkedBook",
                })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "OrphanBook" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @exhaustive {
                            Author {
                                name
                                book(order: {publisher: {yearOpened: ASC}}) {
                                    title
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "book": [{ "title": "OrphanBook" }, { "title": "LinkedBook" }],
                            "name": "John",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-excluded")]
fn query_with_nested_order_by_relation_field_with_asc_and_limit_excludes_orphans() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        title: String
                        author: Author
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        establishedYear: Int @index
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2020" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2010" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2000" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "OrphanBook" })),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_0"),
                    "establishedYear": 2020,
                    "name": "Publisher2020",
                })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_1"),
                    "establishedYear": 2010,
                    "name": "Publisher2010",
                })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_2"),
                    "establishedYear": 2000,
                    "name": "Publisher2000",
                })),
                common: Common {
                    capture: captures([("doc2_2", "/docID")]),
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
                                published(order: {publisher: {establishedYear: ASC}}, limit: 2) {
                                    title
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John",
                            "published": [{ "title": "Book2000" }, { "title": "Book2010" }],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Author {
                                name
                                published(order: {publisher: {establishedYear: ASC}}, limit: 2) {
                                    title
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "docFetches": 2, "indexFetches": 2 }),
                    json!(["subType", "subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-excluded")]
fn query_with_nested_order_by_relation_field_with_asc_and_limit_recursive_explain() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        title: String
                        author: Author
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        establishedYear: Int @index
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2020" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2010" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2000" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_0"),
                    "establishedYear": 2020,
                    "name": "Publisher2020",
                })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_1"),
                    "establishedYear": 2010,
                    "name": "Publisher2010",
                })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_2"),
                    "establishedYear": 2000,
                    "name": "Publisher2000",
                })),
                common: Common {
                    capture: captures([("doc2_2", "/docID")]),
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
                                published(order: {publisher: {establishedYear: ASC}}, limit: 2) {
                                    title
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John",
                            "published": [{ "title": "Book2000" }, { "title": "Book2010" }],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Author {
                                name
                                published(order: {publisher: {establishedYear: ASC}}, limit: 2) {
                                    title
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "docFetches": 2, "indexFetches": 2 }),
                    json!(["subType", "subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-excluded")]
fn query_with_nested_order_by_relation_field_with_desc_and_limit_excludes_orphans() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        title: String
                        author: Author
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        establishedYear: Int @index
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2020" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2010" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2000" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "OrphanBook" })),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_0"),
                    "establishedYear": 2020,
                    "name": "Publisher2020",
                })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_1"),
                    "establishedYear": 2010,
                    "name": "Publisher2010",
                })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_2"),
                    "establishedYear": 2000,
                    "name": "Publisher2000",
                })),
                common: Common {
                    capture: captures([("doc2_2", "/docID")]),
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
                                published(order: {publisher: {establishedYear: DESC}}, limit: 2) {
                                    title
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John",
                            "published": [{ "title": "Book2020" }, { "title": "Book2010" }],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Author {
                                name
                                published(order: {publisher: {establishedYear: DESC}}, limit: 2) {
                                    title
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "docFetches": 2, "indexFetches": 2 }),
                    json!(["subType", "subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-excluded")]
fn query_with_nested_order_by_relation_field_with_desc_and_limit_recursive_explain() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        title: String
                        author: Author
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        establishedYear: Int @index
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2020" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2010" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2000" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "OrphanBook" })),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_0"),
                    "establishedYear": 2020,
                    "name": "Publisher2020",
                })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_1"),
                    "establishedYear": 2010,
                    "name": "Publisher2010",
                })),
                common: Common {
                    capture: captures([("doc2_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({
                    "book": m::r("doc1_2"),
                    "establishedYear": 2000,
                    "name": "Publisher2000",
                })),
                common: Common {
                    capture: captures([("doc2_2", "/docID")]),
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
                                published(order: {publisher: {establishedYear: DESC}}, limit: 2) {
                                    title
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John",
                            "published": [{ "title": "Book2020" }, { "title": "Book2010" }],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Author {
                                name
                                published(order: {publisher: {establishedYear: DESC}}, limit: 2) {
                                    title
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 2 }),
                    json!(["subType", "subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-exhaustive")]
fn query_with_order_by_relation_field_exhaustive_with_parent_primary_asc_should_include_orphans() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        rating: Int @index
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "rating": 5, "title": "Book1" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "OrphanPublisher" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "book": m::r("doc0_0"), "name": "LinkedPublisher" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @exhaustive {
                            Publisher(order: {book: {rating: ASC}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Publisher": [{ "name": "OrphanPublisher" }, { "name": "LinkedPublisher" }],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  @exhaustive {
                            Publisher(order: {book: {rating: ASC}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "docFetches": 1, "indexFetches": 1 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-exhaustive")]
fn query_with_order_by_relation_field_exhaustive_with_parent_primary_desc_should_include_orphans() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        rating: Int @index
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "rating": 5, "title": "Book1" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "OrphanPublisher" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "book": m::r("doc0_0"), "name": "LinkedPublisher" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @exhaustive {
                            Publisher(order: {book: {rating: DESC}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Publisher": [{ "name": "LinkedPublisher" }, { "name": "OrphanPublisher" }],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  @exhaustive {
                            Publisher(order: {book: {rating: DESC}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "docFetches": 1, "indexFetches": 1 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-exhaustive")]
fn query_with_order_by_relation_field_exhaustive_with_parent_secondary_asc_should_include_orphans()
{
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        establishedYear: Int @index
                        book: Book @primary
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Book1" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Book2" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "book": m::r("doc0_0"), "establishedYear": 2020, "name": "Publisher1" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @exhaustive {
                            Book(order: {publisher: {establishedYear: ASC}}) {
                                title
                            }
                        }
                "#.into(),
                expected_data: json!({ "Book": [{ "title": "Book2" }, { "title": "Book1" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  @exhaustive {
                            Book(order: {publisher: {establishedYear: ASC}}) {
                                title
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan_at(json!({ "docFetches": 1, "indexFetches": 1 }), json!(["subType"]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-exhaustive")]
fn query_with_order_by_relation_field_exhaustive_with_parent_secondary_desc_should_include_orphans()
{
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        establishedYear: Int @index
                        book: Book @primary
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Book1" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Book2" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "book": m::r("doc0_0"), "establishedYear": 2020, "name": "Publisher1" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @exhaustive {
                            Book(order: {publisher: {establishedYear: DESC}}) {
                                title
                            }
                        }
                "#.into(),
                expected_data: json!({ "Book": [{ "title": "Book1" }, { "title": "Book2" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  @exhaustive {
                            Book(order: {publisher: {establishedYear: DESC}}) {
                                title
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan_at(json!({ "docFetches": 1, "indexFetches": 1 }), json!(["subType"]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-excluded")]
fn query_with_order_by_relation_field_with_parent_primary_asc_excludes_orphans() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        rating: Int @index
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "rating": 5, "title": "Book1" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "OrphanPublisher" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "book": m::r("doc0_0"), "name": "LinkedPublisher" })),
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
                            Publisher(order: {book: {rating: ASC}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "Publisher": [{ "name": "LinkedPublisher" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Publisher(order: {book: {rating: ASC}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "docFetches": 1, "indexFetches": 1 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-excluded")]
fn query_with_order_by_relation_field_with_parent_secondary_asc_excludes_orphans() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        establishedYear: Int @index
                        book: Book @primary
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Book1" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Book2" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "book": m::r("doc0_0"), "establishedYear": 2020, "name": "Publisher1" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            Book(order: {publisher: {establishedYear: ASC}}) {
                                title
                            }
                        }
                "#.into(),
                expected_data: json!({ "Book": [{ "title": "Book1" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Book(order: {publisher: {establishedYear: ASC}}) {
                                title
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan_at(json!({ "docFetches": 1, "indexFetches": 1 }), json!(["subType"]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-exhaustive")]
fn query_with_order_by_relation_field_with_some_docs_without_relation_should_include_all() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        publisher: Publisher
                    }
                    type Publisher {
                        year: Int @index
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Book1" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Book2" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "book": m::r("doc0_0"), "year": 2020 })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @exhaustive {
                            Book(order: {publisher: {year: ASC}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "Book": [{ "name": "Book2" }, { "name": "Book1" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  @exhaustive {
                            Book(order: {publisher: {year: ASC}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "docFetches": 1, "indexFetches": 1 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-order")]
fn query_with_order_on_one_to_many_with_index_on_order_field_ascending_should_order() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        rating: Float @index
                        author: Author
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 4.5 })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 4.9 })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 4.2 })),
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
                            Author {
                                name
                                published(order: {rating: ASC}) {
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John",
                            "published": [{ "rating": 4.2 }, { "rating": 4.5 }, { "rating": 4.9 }],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Author {
                                name
                                published(order: {rating: ASC}) {
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 3 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-order")]
fn query_with_order_on_one_to_many_with_index_on_order_field_ascending_with_limit_should_order_and_limit(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        rating: Float @index
                        author: Author
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 4.5 })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 4.9 })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 4.2 })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            Author {
                                name
                                published(order: {rating: ASC}, limit: 1) {
                                    rating
                                }
                            }
                        }
                "#.into(),
                expected_data: json!({ "Author": [{ "name": "John", "published": [{ "rating": 4.2 }] }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Author {
                                name
                                published(order: {rating: ASC}, limit: 1) {
                                    rating
                                }
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan_at(json!({ "indexFetches": 1 }), json!(["subType"]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-order")]
fn query_with_order_on_one_to_many_with_index_on_order_field_descending_should_order() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        rating: Float @index
                        author: Author
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 4.5 })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 4.9 })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 4.2 })),
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
                            Author {
                                name
                                published(order: {rating: DESC}) {
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John",
                            "published": [{ "rating": 4.9 }, { "rating": 4.5 }, { "rating": 4.2 }],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Author {
                                name
                                published(order: {rating: DESC}) {
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 3 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-order")]
fn query_with_order_on_one_to_many_with_multiple_authors_and_index_on_relation_should_order_each_authors_books(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        title: String
                        rating: Float @index
                        author: Author @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "author": m::r("doc0_0"), "rating": 3.5, "title": "Book A1" }),
                ),
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
                    json!({ "author": m::r("doc0_0"), "rating": 4.8, "title": "Book A2" }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "author": m::r("doc0_1"), "rating": 4.0, "title": "Book B1" }),
                ),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "author": m::r("doc0_1"), "rating": 2.5, "title": "Book B2" }),
                ),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            Author(order: {name: ASC}) {
                                name
                                published(order: {rating: DESC}) {
                                    title
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "Alice",
                            "published": [
                                { "rating": 4.8, "title": "Book A2" },
                                { "rating": 3.5, "title": "Book A1" },
                            ],
                        },
                        {
                            "name": "Bob",
                            "published": [
                                { "rating": 4.0, "title": "Book B1" },
                                { "rating": 2.5, "title": "Book B2" },
                            ],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Author(order: {name: ASC}) {
                                name
                                published(order: {rating: DESC}) {
                                    title
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 4 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-order")]
fn query_with_order_on_one_to_many_with_multiple_authors_should_order_each_authors_books() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        title: String
                        rating: Float @index
                        author: Author
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "author": m::r("doc0_0"), "rating": 3.5, "title": "Book A1" }),
                ),
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
                    json!({ "author": m::r("doc0_0"), "rating": 4.8, "title": "Book A2" }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "author": m::r("doc0_1"), "rating": 4.0, "title": "Book B1" }),
                ),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "author": m::r("doc0_1"), "rating": 2.5, "title": "Book B2" }),
                ),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            Author(order: {name: ASC}) {
                                name
                                published(order: {rating: DESC}) {
                                    title
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "Alice",
                            "published": [
                                { "rating": 4.8, "title": "Book A2" },
                                { "rating": 3.5, "title": "Book A1" },
                            ],
                        },
                        {
                            "name": "Bob",
                            "published": [
                                { "rating": 4.0, "title": "Book B1" },
                                { "rating": 2.5, "title": "Book B2" },
                            ],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Author(order: {name: ASC}) {
                                name
                                published(order: {rating: DESC}) {
                                    title
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 8 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-order")]
fn query_with_order_on_one_to_many_with_parent_filter_on_relation_and_sub_order_should_order_children(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        title: String
                        rating: Float @index
                        author: Author
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "author": m::r("doc0_0"), "rating": 4.8, "title": "Book A1" }),
                ),
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
                    json!({ "author": m::r("doc0_0"), "rating": 3.5, "title": "Book A2" }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "author": m::r("doc0_1"), "rating": 2.5, "title": "Book B1" }),
                ),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "author": m::r("doc0_1"), "rating": 3.0, "title": "Book B2" }),
                ),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            Author(filter: {published: {rating: {_geq: 4.0}}}) {
                                name
                                published(order: {rating: DESC}) {
                                    title
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "Alice",
                            "published": [
                                { "rating": 4.8, "title": "Book A1" },
                                { "rating": 3.5, "title": "Book A2" },
                            ],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Author(filter: {published: {rating: {_geq: 4.0}}}) {
                                name
                                published(order: {rating: DESC}) {
                                    title
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({
                        "docFetches": 5,
                        "fieldFetches": 15,
                        "indexFetches": 5,
                        "iterations": 5,
                    }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-order")]
fn query_with_order_on_one_to_many_with_sub_filter_and_order_and_relation_index_should_filter_then_order(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        title: String
                        rating: Float @index
                        author: Author @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 3.5, "title": "Book1" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 4.8, "title": "Book2" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 4.2, "title": "Book3" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 4.4, "title": "Book4" })),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
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
                                published(filter: {rating: {_geq: 4.0}}, order: {rating: DESC}) {
                                    title
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John",
                            "published": [
                                { "rating": 4.8, "title": "Book2" },
                                { "rating": 4.4, "title": "Book4" },
                                { "rating": 4.2, "title": "Book3" },
                            ],
                        },
                        { "name": "Fred", "published": [] },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Author {
                                name
                                published(filter: {rating: {_geq: 4.0}}, order: {rating: DESC}) {
                                    title
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 6 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-order")]
fn query_with_order_on_one_to_many_with_sub_filter_and_order_on_same_indexed_field_should_filter_then_order_asc(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        title: String
                        rating: Float @index
                        author: Author
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
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
                    json!({ "author": m::r("doc0_0"), "rating": 2.0, "title": "Low Rated" }),
                ),
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
                    json!({ "author": m::r("doc0_0"), "rating": 4.5, "title": "Mid Rated" }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "author": m::r("doc0_0"), "rating": 4.9, "title": "High Rated" }),
                ),
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
                            Author {
                                name
                                published(filter: {rating: {_gt: 3.0}}, order: {rating: ASC}) {
                                    title
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John",
                            "published": [
                                { "rating": 4.5, "title": "Mid Rated" },
                                { "rating": 4.9, "title": "High Rated" },
                            ],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Author {
                                name
                                published(filter: {rating: {_gt: 3.0}}, order: {rating: ASC}) {
                                    title
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "docFetches": 2, "indexFetches": 2 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-order")]
fn query_with_order_on_one_to_many_with_sub_filter_and_order_on_same_indexed_field_should_filter_then_order_desc(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        published: [Book]
                    }
                    type Book {
                        title: String
                        rating: Float @index
                        author: Author
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John" })),
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
                    json!({ "author": m::r("doc0_0"), "rating": 2.0, "title": "Low Rated" }),
                ),
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
                    json!({ "author": m::r("doc0_0"), "rating": 4.5, "title": "Mid Rated" }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "author": m::r("doc0_0"), "rating": 4.9, "title": "High Rated" }),
                ),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(
                    json!({ "author": m::r("doc0_0"), "rating": 5.0, "title": "Top Rated" }),
                ),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
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
                                published(filter: {rating: {_geq: 4.5}}, order: {rating: DESC}) {
                                    title
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John",
                            "published": [
                                { "rating": 5.0, "title": "Top Rated" },
                                { "rating": 4.9, "title": "High Rated" },
                                { "rating": 4.5, "title": "Mid Rated" },
                            ],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Author {
                                name
                                published(filter: {rating: {_geq: 4.5}}, order: {rating: DESC}) {
                                    title
                                    rating
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "docFetches": 3, "indexFetches": 3 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
