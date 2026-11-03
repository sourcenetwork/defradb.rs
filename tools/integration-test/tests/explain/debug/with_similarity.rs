//! Generated from the conformance scenarios of tests/integration/explain/debug/with_similarity_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.debug.special_nodes")]
fn debug_explain_request_with_with_similarity() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        pointsList: [Float64!]
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "pointsList": [2.0, 4.0, 1.0] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: debug) {
                                        User {
                                            name
                                            SIMILARITY(pointsList: {vector: [1, 2, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "explain": {
                        "operationNode": [
                            {
                                "selectTopNode": { "similarityNode": { "selectNode": { "scanNode": {} } } },
                            },
                        ],
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
