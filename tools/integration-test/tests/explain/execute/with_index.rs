//! Generated from the conformance scenarios of tests/integration/explain/execute/with_index_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.execute.index")]
fn execute_explain_with_index_on_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice", "age": 25 })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 30 })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        User(filter: {age: {_eq: 30}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "explain": {
                        "executionSuccess": true,
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "selectNode": {
                                        "filterMatches": 2,
                                        "iterations": 3,
                                        "scanNode": {
                                            "docFetches": 2,
                                            "fieldFetches": 4,
                                            "indexFetches": 2,
                                            "iterations": 3,
                                        },
                                    },
                                },
                            },
                        ],
                        "planExecutions": 2,
                        "sizeOfResult": 1,
                    },
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
#[mantra_macros::req_test("go.explain.execute.index")]
fn execute_explain_with_index_on_order() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice", "age": 25 })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 35 })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        User(order: {age: ASC}) {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "explain": {
                        "executionSuccess": true,
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "selectNode": {
                                        "filterMatches": 3,
                                        "iterations": 4,
                                        "scanNode": {
                                            "docFetches": 3,
                                            "fieldFetches": 6,
                                            "indexFetches": 3,
                                            "iterations": 4,
                                        },
                                    },
                                },
                            },
                        ],
                        "planExecutions": 2,
                        "sizeOfResult": 1,
                    },
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
#[mantra_macros::req_test("go.explain.execute.index")]
fn execute_explain_with_index_on_relation_order() {
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
                collection: "Book".into(),
                doc_map: Some(json!({ "rating": 3, "title": "Book2" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "book": m::r("doc0_0"), "name": "Publisher1" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "book": m::r("doc0_1"), "name": "Publisher2" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) @exhaustive {
                                        Publisher(order: {book: {rating: DESC}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "explain": {
                        "executionSuccess": true,
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "selectNode": {
                                        "filterMatches": 2,
                                        "iterations": 3,
                                        "typeIndexJoin": {
                                            "iterations": 3,
                                            "sequenceNode": [
                                                {
                                                    "typeJoinOne": {
                                                        "root": {
                                                            "scanNode": {
                                                                "docFetches": 2,
                                                                "fieldFetches": 4,
                                                                "indexFetches": 2,
                                                                "iterations": 4,
                                                            },
                                                        },
                                                        "subType": {
                                                            "selectTopNode": {
                                                                "selectNode": {
                                                                    "filterMatches": 2,
                                                                    "iterations": 3,
                                                                    "scanNode": {
                                                                        "docFetches": 2,
                                                                        "fieldFetches": 4,
                                                                        "indexFetches": 2,
                                                                        "iterations": 3,
                                                                    },
                                                                },
                                                            },
                                                        },
                                                    },
                                                },
                                                {
                                                    "orphanNode": {
                                                        "docFetches": 0,
                                                        "fieldFetches": 0,
                                                        "indexFetches": 0,
                                                        "iterations": 1,
                                                    },
                                                },
                                            ],
                                        },
                                    },
                                },
                            },
                        ],
                        "planExecutions": 2,
                        "sizeOfResult": 1,
                    },
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
#[mantra_macros::req_test("go.explain.execute.index")]
fn execute_explain_with_index_on_subquery_nested_relation_order() {
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
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2020" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "title": "Book2010" })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
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
                common: Common { capture: captures([("doc2_0", "/docID")]), ..Default::default() },
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
                common: Common { capture: captures([("doc2_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        Author {
                                            name
                                            published(order: {publisher: {establishedYear: DESC}}, limit: 2) {
                                                title
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "explain": {
                        "executionSuccess": true,
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "selectNode": {
                                        "filterMatches": 1,
                                        "iterations": 2,
                                        "typeIndexJoin": {
                                            "iterations": 2,
                                            "typeJoinMany": {
                                                "root": {
                                                    "scanNode": {
                                                        "docFetches": 1,
                                                        "fieldFetches": 1,
                                                        "indexFetches": 0,
                                                        "iterations": 2,
                                                    },
                                                },
                                                "subType": {
                                                    "selectTopNode": {
                                                        "limitNode": {
                                                            "iterations": 3,
                                                            "selectNode": {
                                                                "filterMatches": 2,
                                                                "iterations": 2,
                                                                "typeIndexJoin": {
                                                                    "iterations": 2,
                                                                    "typeJoinOne": {
                                                                        "root": {
                                                                            "scanNode": {
                                                                                "docFetches": 2,
                                                                                "fieldFetches": 4,
                                                                                "indexFetches": 0,
                                                                                "iterations": 2,
                                                                            },
                                                                        },
                                                                        "subType": {
                                                                            "selectTopNode": {
                                                                                "selectNode": {
                                                                                    "filterMatches": 2,
                                                                                    "iterations": 2,
                                                                                    "scanNode": {
                                                                                        "docFetches": 2,
                                                                                        "fieldFetches": 6,
                                                                                        "indexFetches": 2,
                                                                                        "iterations": 2,
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
                        "planExecutions": 2,
                        "sizeOfResult": 1,
                    },
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
