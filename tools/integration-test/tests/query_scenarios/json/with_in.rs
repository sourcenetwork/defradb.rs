//! Generated from the conformance scenarios of tests/integration/query/json/with_in_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.json.in")]
fn query_json_with_in_filter_should_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        custom: JSON
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "custom": { "tree": "maple", "age": 250 } })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Andy", "custom": { "tree": "oak", "age": 450 } })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {custom: {_in: [{tree:"oak",age:450}]}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Andy" }] }),
                common: Common {
                    compare: Some(true),
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
