//! Generated from the conformance scenarios of tests/integration/query/simple/with_group_count_min_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.group.aggregate.of-other")]
fn query_simple_with_group_by_string_with_inner_group_boolean_and_min_of_count_succeeds() {
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
                doc_map: Some(json!({ "Name": "John", "Age": 25, "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 32, "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 34, "Verified": false })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Carlo", "Age": 55, "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 19, "Verified": false })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            MIN(GROUP: {field: COUNT})
                                            GROUP (groupBy: [Verified]){
                                                Verified
                                                COUNT(GROUP: {})
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "GROUP": [
                                { "COUNT": 2, "Verified": true },
                                { "COUNT": 1, "Verified": false },
                            ],
                            "MIN": 1,
                            "Name": "John",
                        },
                        {
                            "GROUP": [{ "COUNT": 1, "Verified": false }],
                            "MIN": 1,
                            "Name": "Alice",
                        },
                        {
                            "GROUP": [{ "COUNT": 1, "Verified": true }],
                            "MIN": 1,
                            "Name": "Carlo",
                        },
                    ],
                }),
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
