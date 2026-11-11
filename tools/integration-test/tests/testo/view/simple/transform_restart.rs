//! Generated from the conformance scenarios of tests/integration/view/simple/transform_restart_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{
    AddCollection, AddDoc, AddLens, AddView, NewNode, Request, Restart,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.view.transforms")]
fn view_simple_with_transform_and_restart() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                database_dir: Some("".into()),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddLens {
                lens: Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "fullName", "src": "name" }),
                        path: "bin/lenses/copy.wasm".into(),
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
                        name
                    }
                "#
                .into(),
                sdl: r#"
                    type UserView @materialized(if: false) {
                        fullName: String
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
            Restart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        UserView {
                            fullName
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "UserView": [{ "fullName": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
