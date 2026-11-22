//! Rust execution explains name the index behind an index-backed scan.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.explain.index_name")]
fn a_rust_index_scan_names_its_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp]),
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
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice", "age": 25 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob", "age": 30 })),
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
                        "operationNode": [{
                            "selectTopNode": {
                                "selectNode": {
                                    "filterMatches": 2,
                                    "iterations": 3,
                                    "scanNode": {
                                        "docFetches": 2,
                                        "fieldFetches": 4,
                                        "indexFetches": 2,
                                        "indexName": "User_age_ASC",
                                        "iterations": 3,
                                    },
                                },
                            },
                        }],
                        "planExecutions": 2,
                        "sizeOfResult": 1,
                    },
                }),
                common: Common::default(),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
