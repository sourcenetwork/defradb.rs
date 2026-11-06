//! Generated from the conformance scenarios of tests/integration/view/simple/with_transform_aggregate_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{AddCollection, AddDoc, AddLens, AddView, Request};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.view.transforms")]
fn view_simple_with_transform_aggregate() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "stddev", "src": "age" }),
                        path: "bin/lenses/std-dev.wasm".into(),
                    }]),
                },
                common: Common {
                    capture: captures([("lens0", "/lensID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddView {
                query: r#"
                    User {
                        age
                    }
                "#
                .into(),
                sdl: r#"
                    type UserStdDev @materialized(if: false) {
                        stddev: String
                    }
                "#
                .into(),
                transform_cid: Some(Text::concat([
                    Text::from(""),
                    Text::reference("lens0"),
                    Text::from(""),
                ])),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 26 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 34 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        UserStdDev {
                            stddev
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "UserStdDev": [{ "stddev": 3.265986323710904 }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
