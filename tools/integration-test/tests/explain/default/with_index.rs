//! Generated from the conformance scenarios of tests/integration/explain/default/with_index_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn default_explain_with_index_on_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int @index
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain {
                                        User(filter: {age: {_eq: 30}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::explain_pattern(json!({
                    "explain": {
                        "operationNode": [{ "selectTopNode": { "selectNode": { "scanNode": {} } } }],
                    },
                }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn default_explain_with_index_on_order() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int @index
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain {
                                        User(order: {age: ASC}) {
                                            name
                                            age
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::explain_pattern(json!({
                    "explain": {
                        "operationNode": [{ "selectTopNode": { "selectNode": { "scanNode": {} } } }],
                    },
                }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn default_explain_with_index_on_subquery_nested_relation_order() {
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
            Request {
                request: r#"
                    query @explain {
                                        Author {
                                            name
                                            published(order: {publisher: {establishedYear: DESC}}, limit: 2) {
                                                title
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::explain_pattern(json!({
                    "explain": {
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "selectNode": {
                                        "typeIndexJoin": {
                                            "root": { "scanNode": {} },
                                            "subType": {
                                                "selectTopNode": {
                                                    "limitNode": {
                                                        "selectNode": {
                                                            "typeIndexJoin": {
                                                                "root": { "scanNode": {} },
                                                                "subType": {
                                                                    "selectTopNode": {
                                                                        "selectNode": { "scanNode": {} },
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
                }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
