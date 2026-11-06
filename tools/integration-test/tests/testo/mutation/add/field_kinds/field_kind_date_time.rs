//! Generated from the conformance scenarios of tests/integration/mutation/add/field_kinds/field_kind_date_time_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.utc_now")]
fn mutation_add_with_date_time_sets_two_equal_utc_now_values() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        created: DateTime
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        bob: add_User(input: { name: "Bob", created: UTC_NOW }) {
                                            created
                                        }

                                        alice: add_User(input: { name: "Alice", created: UTC_NOW }) {
                                            created
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "alice": [{ "created": m::same("timestampMatcher") }],
                    "bob": [{ "created": m::same("timestampMatcher") }],
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
fn mutation_add_field_kinds_with_date_time_with_utc_now() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        time: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    mutation {
                                        add_User(input: {time: UTC_NOW}) {
                                            time
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "add_User": [{ "time": m::current_timestamp() }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.field_kinds.other_scalars")]
fn mutation_add_field_kinds_with_date_time() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        time: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "time": "2017-07-23T03:46:56.000Z" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            time
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "time": "2017-07-23T03:46:56Z" }] }),
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
#[mantra_macros::req_test("go.mutation.field_kinds.other_scalars")]
fn mutation_add_field_kinds_with_date_times_nano_seconds_appart() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        time: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "time": "2017-07-23T03:46:56.000Z" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "time": "2017-07-23T03:46:56.000000001Z" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "time": "2017-07-23T03:46:56.000000002Z" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            time
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        { "time": "2017-07-23T03:46:56Z" },
                        { "time": "2017-07-23T03:46:56.000000002Z" },
                        { "time": "2017-07-23T03:46:56.000000001Z" },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    unordered_documents: vec!["/data/User".into()],
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
#[mantra_macros::req_test("go.mutation.field_kinds.nillable_arrays")]
fn mutation_add_field_kinds_with_nillable_date_time_array_from_string_slice() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        times: [DateTime]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "times": ["2017-07-23T03:46:56Z", "2018-08-24T04:47:57Z"] })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            times
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [{ "times": ["2017-07-23T03:46:56Z", "2018-08-24T04:47:57Z"] }],
                }),
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
#[mantra_macros::req_test("go.mutation.field_kinds.nillable_arrays")]
fn mutation_add_field_kinds_with_nillable_date_time_array_from_typed_slice() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        times: [DateTime]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "times": ["2017-07-23T03:46:56Z", "2018-08-24T04:47:57Z"] })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            times
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [{ "times": ["2017-07-23T03:46:56Z", "2018-08-24T04:47:57Z"] }],
                }),
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
#[mantra_macros::req_test("go.mutation.field_kinds.nillable_arrays")]
fn mutation_add_field_kinds_with_nillable_date_time_array_with_nil_element() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        times: [DateTime]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "times": ["2017-07-23T03:46:56Z", null, "2018-08-24T04:47:57Z"] }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            times
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        {
                            "times": ["2017-07-23T03:46:56Z", null, "2018-08-24T04:47:57Z"],
                        },
                    ],
                }),
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
#[mantra_macros::req_test("go.mutation.field_kinds.null_scalars")]
fn mutation_add_field_kinds_with_nillable_date_time_nil() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        time: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "time": null })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            time
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "time": null }] }),
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
#[mantra_macros::req_test("go.mutation.field_kinds.non_nillable_arrays")]
fn mutation_add_field_kinds_with_non_nillable_date_time_array_from_any_slice() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        times: [DateTime!]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "times": ["2017-07-23T03:46:56Z", "2018-08-24T04:47:57Z"] })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            times
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [{ "times": ["2017-07-23T03:46:56Z", "2018-08-24T04:47:57Z"] }],
                }),
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
#[mantra_macros::req_test("go.mutation.field_kinds.non_nillable_arrays")]
fn mutation_add_field_kinds_with_non_nillable_date_time_array_from_string_slice() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        times: [DateTime!]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "times": ["2017-07-23T03:46:56Z", "2018-08-24T04:47:57Z"] })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            times
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [{ "times": ["2017-07-23T03:46:56Z", "2018-08-24T04:47:57Z"] }],
                }),
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
#[mantra_macros::req_test("go.mutation.field_kinds.non_nillable_arrays")]
fn mutation_add_field_kinds_with_non_nillable_date_time_array_from_typed_slice() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        times: [DateTime!]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "times": ["2017-07-23T03:46:56Z", "2018-08-24T04:47:57Z"] })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            times
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [{ "times": ["2017-07-23T03:46:56Z", "2018-08-24T04:47:57Z"] }],
                }),
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
#[mantra_macros::req_test("go.mutation.field_kinds.other_scalars")]
fn mutation_add_field_kinds_with_non_nillable_date_time() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        time: DateTime!
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "time": "2017-07-23T03:46:56Z" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            time
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "time": "2017-07-23T03:46:56Z" }] }),
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
