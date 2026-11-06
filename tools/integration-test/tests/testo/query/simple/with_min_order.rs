//! Generated from the conformance scenarios of tests/integration/query/simple/with_min_order_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.order.aggregate-alias")]
fn query_simple_with_min_with_order_succeeds() {
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
                doc_map: Some(json!({ "Age": 30, "HeightM": 1.8 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Age": 25, "HeightM": 1.6 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: {_alias: {total: DESC}}) {
                                            total: MIN(HeightM: {}, Age: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "total": 1.8 }, { "total": 1.6 }] }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: {_alias: {total: ASC}}) {
                                            total: MIN(HeightM: {}, Age: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "total": 1.6 }, { "total": 1.8 }] }),
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
