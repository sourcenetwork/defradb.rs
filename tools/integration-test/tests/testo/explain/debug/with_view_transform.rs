//! Generated from the conformance scenarios of tests/integration/explain/debug/with_view_transform_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{AddCollection, AddLens, AddView, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.debug.special_nodes")]
fn debug_explain_request_with_view_with_transform() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddLens {
                lens: Lens {
                    lenses: Some(vec![
                        LensModule {
                            path: "bin/lenses/copy.wasm".into(),
                            arguments: json!({ "dst": "fullName", "src": "name" }),
                        },
                    ]),
                },
                common: Common { capture: captures([("lens0", "/lensID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    User {
                        name
                    }
                "#.into(),
                sdl: r#"
                    type UserView @materialized(if: false) {
                        fullName: String
                    }
                "#.into(),
                transform_cid: Some(Text::reference("lens0")),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: debug) {
                                        UserView {
                                            fullName
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::explain_pattern(json!({
                    "explain": {
                        "operationNode": [
                            {
                                "selectTopNode": {
                                    "selectNode": {
                                        "viewNode": {
                                            "lensNode": {
                                                "selectTopNode": { "selectNode": { "scanNode": {} } },
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
