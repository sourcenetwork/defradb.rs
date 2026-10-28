//! Generated from the conformance scenarios of tests/integration/mutation/upsert/date_time_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.utc_now")]
fn mutation_upsert_with_date_time_field_with_utc_now_should_be_equal() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        created_at: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "created_at": "2011-07-23T01:11:11-05:00" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        john: upsert_Users(
                                            filter: {name: {_eq: "John"}},
                                            add: {name: "John", created_at: UTC_NOW},
                                            update: {created_at: UTC_NOW}
                                        ) {
                                            created_at
                                        }
                                        chris: upsert_Users(
                                            filter: {name: {_eq: "Chris"}},
                                            add: {name: "Chris", created_at: UTC_NOW},
                                            update: {created_at: UTC_NOW}
                                        ) {
                                            created_at
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "chris": [{ "created_at": m::same("timestamp") }],
                    "john": [{ "created_at": m::same("timestamp") }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
