//! Generated from the conformance scenarios of tests/integration/query/simple/with_restart_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, NewNode, Request, Restart};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.select.fields")]
fn query_simple_with_restart() {
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
                    type Users {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Restart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 30 })),
                ..Default::default()
            }
            .into(),
            Restart {
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                       Users {
                                           name
                                           age
                                       }
                                   }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "age": 30, "name": "Shahzad" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
