//! Generated from the conformance scenarios of tests/integration/query/simple/with_operation_name_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.graphql.operation-name")]
fn query_simple_multiple_operations_with_no_operation_name_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 40 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 21 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query UsersByName {
                                        Users {
                                            Name
                                        }
                                    }
                                    query UsersByAge {
                                        Users {
                                            Age
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(
                    "Must provide operation name if query contains multiple operations.".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.graphql.operation-name")]
fn query_simple_multiple_operations_with_operation_name() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 40 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 21 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query UsersByName {
                                        Users {
                                            Name
                                        }
                                    }
                                    query UsersByAge {
                                        Users {
                                            Age
                                        }
                                    }
                "#
                .into(),
                operation_name: Some("UsersByName".into()),
                expected_data: json!({ "Users": [{ "Name": "Alice" }, { "Name": "Bob" }] }),
                common: Common {
                    compare: Some(true),
                    unordered_documents: vec!["/data/Users".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query UsersByName {
                                        Users {
                                            Name
                                        }
                                    }
                                    query UsersByAge {
                                        Users {
                                            Age
                                        }
                                    }
                "#
                .into(),
                operation_name: Some("UsersByAge".into()),
                expected_data: json!({ "Users": [{ "Age": 40 }, { "Age": 21 }] }),
                common: Common {
                    compare: Some(true),
                    unordered_documents: vec!["/data/Users".into()],
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
