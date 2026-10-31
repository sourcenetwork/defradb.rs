//! Generated from the conformance scenarios of tests/integration/mutation/update/field_kinds/date_time_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.update.set_null")]
fn mutation_update_if_date_time_field_set_to_null_should_be_nil() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        created_at: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "created_at": "2011-07-23T01:11:11-05:00" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "created_at": null }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            created_at
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "created_at": null }] }),
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

#[test]
#[mantra_macros::req_test("go.mutation.update.basic")]
fn mutation_update_with_date_time_field_multiple_docs() {
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
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "created_at": "2021-07-23T02:22:22-05:00" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        update_Users(input: {created_at: "2031-07-23T03:23:23Z"}) {
                                            name
                                            created_at
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "update_Users": [
                        { "created_at": "2031-07-23T03:23:23Z", "name": "John" },
                        { "created_at": "2031-07-23T03:23:23Z", "name": "Fred" },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    unordered_documents: vec!["/data/update_Users".into()],
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

#[test]
#[mantra_macros::req_test("go.mutation.utc_now")]
fn mutation_update_with_date_time_field_with_utc_now_should_be_equal() {
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
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "name": "Chris", "created_at": "2012-07-23T01:11:11-05:00" }),
                ),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        john: update_Users(
                                            filter: { name: { _eq: "John" } },
                                            input: { created_at: UTC_NOW }
                                        ) {
                                            created_at
                                        }
                                        chris: update_Users(
                                            filter: { name: { _eq: "Chris" } },
                                            input: { created_at: UTC_NOW }
                                        ) {
                                            created_at
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "chris": [{ "created_at": m::same("timestampMatcher") }],
                    "john": [{ "created_at": m::same("timestampMatcher") }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.utc_now")]
fn mutation_update_with_date_time_field_with_utc_now() {
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
                                        update_Users(
                                            filter: { name: { _eq: "John" } },
                                            input: { created_at: UTC_NOW }
                                        ) {
                                            name
                                            created_at
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "update_Users": [{ "name": "John", "created_at": m::current_timestamp() }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.update.basic")]
fn mutation_update_with_date_time_field() {
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
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "created_at": "2021-07-23T02:22:22-05:00" }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            created_at
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "created_at": "2021-07-23T02:22:22-05:00" }] }),
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
