//! Pins the plan explain shape for an index-backed scalar filter.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.index")]
fn indexed_scan_explains_the_selected_index() {
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
            Request {
                request: r#"
                    query @explain {
                        User(filter: {age: {_eq: 30}}) {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "explain": {
                        "operationNode": [{
                            "selectTopNode": {
                                "selectNode": {
                                    "docID": null,
                                    "filter": null,
                                    "scanNode": {
                                        "collectionID": "bafyreiek4irx5zh2it7sokxwbqud2uanuhkqppnmt2dgan7r4gyafly6wi",
                                        "collectionName": "User",
                                        "filter": {"age": {"_eq": 30}},
                                        "indexFetches": 0,
                                        "indexLimit": null,
                                        "indexName": "User_age_ASC",
                                        "indexOffset": 0,
                                        "prefixes": ["/1"],
                                    },
                                },
                            },
                        }],
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
