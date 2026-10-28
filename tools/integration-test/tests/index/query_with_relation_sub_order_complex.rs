//! Generated from the conformance scenarios of tests/integration/index/query_with_relation_sub_order_complex_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-exhaustive")]
fn query_with_order_by_relation_field_exhaustive_asc_many_books_shows_full_pipeline() {
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
                doc_map: Some(json!({ "title": "Linked-2020" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Linked-2010" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Linked-2000" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-A" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-B" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-C" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-D" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-E" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-F" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-G" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(
                    json!({ "book": m::r("doc0_0"), "establishedYear": 2020, "name": "Pub2020" }),
                ),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(
                    json!({ "book": m::r("doc0_1"), "establishedYear": 2010, "name": "Pub2010" }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(
                    json!({ "book": m::r("doc0_2"), "establishedYear": 2000, "name": "Pub2000" }),
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
                    query @exhaustive {
                            Book(order: {publisher: {establishedYear: ASC}}) {
                                title
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        { "title": "Orphan-A" },
                        { "title": "Orphan-F" },
                        { "title": "Orphan-B" },
                        { "title": "Orphan-E" },
                        { "title": "Orphan-D" },
                        { "title": "Orphan-C" },
                        { "title": "Orphan-G" },
                        { "title": "Linked-2000" },
                        { "title": "Linked-2010" },
                        { "title": "Linked-2020" },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Book".into()],
                    ..Default::default()
                },
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

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-exhaustive")]
fn query_with_order_by_relation_field_exhaustive_asc_with_limit_many_orphans_early_termination() {
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
                doc_map: Some(json!({ "title": "Linked-2020" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Linked-2010" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Linked-2000" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-A" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-B" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-C" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-D" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-E" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-F" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-G" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(
                    json!({ "book": m::r("doc0_0"), "establishedYear": 2020, "name": "Pub2020" }),
                ),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(
                    json!({ "book": m::r("doc0_1"), "establishedYear": 2010, "name": "Pub2010" }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(
                    json!({ "book": m::r("doc0_2"), "establishedYear": 2000, "name": "Pub2000" }),
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
                    query @exhaustive {
                            Book(order: {publisher: {establishedYear: ASC}}, limit: 3) {
                                title
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        { "title": "Orphan-A" },
                        { "title": "Orphan-B" },
                        { "title": "Orphan-C" },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Book".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  @exhaustive {
                            Book(order: {publisher: {establishedYear: ASC}}, limit: 3) {
                                title
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "docFetches": 0, "indexFetches": 0 }),
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
fn query_with_order_by_relation_field_exhaustive_desc_with_limit_many_orphans_skips_orphan_phase() {
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
                doc_map: Some(json!({ "title": "Linked-2020" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Linked-2010" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Linked-2000" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-A" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-B" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-C" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-D" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-E" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-F" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "Orphan-G" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(
                    json!({ "book": m::r("doc0_0"), "establishedYear": 2020, "name": "Pub2020" }),
                ),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(
                    json!({ "book": m::r("doc0_1"), "establishedYear": 2010, "name": "Pub2010" }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(
                    json!({ "book": m::r("doc0_2"), "establishedYear": 2000, "name": "Pub2000" }),
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
                    query @exhaustive {
                            Book(order: {publisher: {establishedYear: DESC}}, limit: 3) {
                                title
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        { "title": "Linked-2020" },
                        { "title": "Linked-2010" },
                        { "title": "Linked-2000" },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  @exhaustive {
                            Book(order: {publisher: {establishedYear: DESC}}, limit: 3) {
                                title
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
