//! Generated from the conformance scenarios of tests/integration/explain/execute/with_orphan_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.execute.orphan")]
fn execute_explain_with_orphan_node_with_primary_parent_reports_metrics() {
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
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "rating": 5, "title": "Book1" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "OrphanPublisher" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "book": m::r("doc0_0"), "name": "LinkedPublisher" })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) @exhaustive {
                                        Publisher(order: {book: {rating: ASC}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::explain_target("orphanNode", false, 0, json!({ "docFetches": 1, "fieldFetches": 1, "indexFetches": 1, "iterations": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.explain.execute.orphan")]
fn execute_explain_with_orphan_node_with_secondary_parent_reports_metrics() {
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
                doc_map: Some(json!({ "title": "OrphanBook" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "title": "LinkedBook" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "book": m::r("doc0_1"), "establishedYear": 2020, "name": "Publisher1" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) @exhaustive {
                                        Book(order: {publisher: {establishedYear: ASC}}) {
                                            title
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::explain_target("orphanNode", false, 0, json!({ "docFetches": 2, "fieldFetches": 2, "indexFetches": 2, "iterations": 3 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
