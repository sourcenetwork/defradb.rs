//! Generated from the conformance scenarios of tests/integration/query/inline_array/with_datetime_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.inline_array.read.datetime")]
fn query_inline_array_with_date_time_empty_list() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Events {
                        name: String
                        scheduledTimes: [DateTime!]
                        optionalTimes: [DateTime]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({ "name": "Meeting", "scheduledTimes": [] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Events {
                                            name
                                            scheduledTimes
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Events": [{ "name": "Meeting", "scheduledTimes": [] }] }),
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
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-reject")]
fn query_inline_array_with_date_time_error_malformed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Events {
                        name: String
                        scheduledTimes: [DateTime!]
                        optionalTimes: [DateTime]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({ "name": "Bad Date", "scheduledTimes": ["not-a-date"] })),
                expected_error: Some("not-a-date".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.filter.datetime")]
fn query_inline_array_with_date_time_filter_all() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Events {
                        name: String
                        scheduledTimes: [DateTime!]
                        optionalTimes: [DateTime]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({
                    "name": "Waitlisted",
                    "scheduledTimes": ["2024-01-15T09:00:00Z", "2024-01-16T09:00:00Z"],
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({
                    "name": "Confirmed",
                    "scheduledTimes": ["2024-01-15T09:00:00Z", "2024-01-15T10:00:00Z"],
                })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Events(filter: {scheduledTimes: {_all: {_eq: "2024-01-15T09:00:00Z"}}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Events": [] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Events(filter: {scheduledTimes: {_all: {_geq: "2024-01-15T09:00:00Z"}}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Events": [{ "name": "Confirmed" }, { "name": "Waitlisted" }] }),
                common: Common { compare: Some(true), unordered_documents: vec!["/data/Events".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.filter.datetime")]
fn query_inline_array_with_date_time_filter_any() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Events {
                        name: String
                        scheduledTimes: [DateTime!]
                        optionalTimes: [DateTime]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({
                    "name": "Morning Event",
                    "scheduledTimes": ["2024-01-15T09:00:00Z", "2024-01-15T10:00:00Z"],
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({
                    "name": "Evening Event",
                    "scheduledTimes": ["2024-01-15T18:00:00Z", "2024-01-15T19:00:00Z"],
                })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Events(filter: {scheduledTimes: {_any: {_eq: "2024-01-15T09:00:00Z"}}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Events": [{ "name": "Morning Event" }] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.filter.datetime")]
fn query_inline_array_with_date_time_filter_none() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Events {
                        name: String
                        scheduledTimes: [DateTime!]
                        optionalTimes: [DateTime]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({ "name": "Future Event", "scheduledTimes": ["2024-02-01T00:00:00Z"] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({ "name": "Past Event", "scheduledTimes": ["2023-01-01T00:00:00Z"] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Events(filter: {scheduledTimes: {_none: {_lt: "2024-01-01T00:00:00Z"}}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Events": [{ "name": "Future Event" }] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.filter.datetime")]
fn query_inline_array_with_date_time_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Events {
                        name: String
                        scheduledTimes: [DateTime!] @index
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({ "name": "Indexed Event", "scheduledTimes": ["2024-06-01T12:00:00Z"] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Events(filter: {scheduledTimes: {_any: {_eq: "2024-06-01T12:00:00Z"}}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Events": [{ "name": "Indexed Event" }] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.read.datetime")]
fn query_inline_array_with_date_time_non_nillable() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Events {
                        name: String
                        scheduledTimes: [DateTime!]
                        optionalTimes: [DateTime]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({
                    "name": "Conference",
                    "scheduledTimes": ["2025-01-01T10:00:00Z", "2025-01-02T14:00:00Z"],
                })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Events {
                                            name
                                            scheduledTimes
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Events": [
                        {
                            "name": "Conference",
                            "scheduledTimes": ["2025-01-01T10:00:00Z", "2025-01-02T14:00:00Z"],
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
#[mantra_macros::req_test("go.query.inline_array.read.datetime")]
fn query_inline_array_with_date_time_not_empty() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Events {
                        name: String
                        scheduledTimes: [DateTime!]
                        optionalTimes: [DateTime]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({
                    "name": "Conference",
                    "scheduledTimes": [
                        "2024-01-15T09:00:00Z",
                        "2024-01-15T14:00:00Z",
                        "2024-01-16T10:00:00Z",
                    ],
                })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Events {
                                            name
                                            scheduledTimes
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Events": [
                        {
                            "name": "Conference",
                            "scheduledTimes": [
                                "2024-01-15T09:00:00Z",
                                "2024-01-15T14:00:00Z",
                                "2024-01-16T10:00:00Z",
                            ],
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
#[mantra_macros::req_test("go.query.inline_array.read.datetime")]
fn query_inline_array_with_date_time_null() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Events {
                        name: String
                        scheduledTimes: [DateTime!]
                        optionalTimes: [DateTime]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({ "name": "Birthday Party", "scheduledTimes": null })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Events {
                                            name
                                            scheduledTimes
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Events": [{ "name": "Birthday Party", "scheduledTimes": null }] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.read.datetime")]
fn query_inline_array_with_date_time_update() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Events {
                        name: String
                        scheduledTimes: [DateTime!]
                        optionalTimes: [DateTime]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({
                    "name": "Rescheduled Event",
                    "scheduledTimes": ["2024-01-01T00:00:00Z"],
                })),
                common: Common { capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Events".into(),
                doc_map: json!({ "scheduledTimes": ["2024-02-01T00:00:00Z", "2024-03-01T00:00:00Z"] }),
                doc_id: Text::reference("doc0"),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Events {
                                            name
                                            scheduledTimes
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Events": [
                        {
                            "name": "Rescheduled Event",
                            "scheduledTimes": ["2024-02-01T00:00:00Z", "2024-03-01T00:00:00Z"],
                        },
                    ],
                }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.read.datetime")]
fn query_inline_array_with_date_time_utc_now() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Events {
                        name: String
                        scheduledTimes: [DateTime!]
                        optionalTimes: [DateTime]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({ "name": "Now Event", "scheduledTimes": ["UTC_NOW"] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Events { name scheduledTimes } }".into(),
                expected_data: json!({
                    "Events": [
                        {
                            "name": "Now Event",
                            "scheduledTimes": [m::current_timestamp()],
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
#[mantra_macros::req_test("go.query.inline_array.read.datetime")]
fn query_inline_array_with_nillable_date_time_all_null() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Events {
                        name: String
                        scheduledTimes: [DateTime!]
                        optionalTimes: [DateTime]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({ "name": "Pending", "optionalTimes": [null, null] })),
                common: Common { capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Events {
                                            name
                                            optionalTimes
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Events": [{ "name": "Pending", "optionalTimes": [null, null] }] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.filter.datetime")]
fn query_inline_array_with_nillable_date_time_filter_any() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Events {
                        name: String
                        scheduledTimes: [DateTime!]
                        optionalTimes: [DateTime]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({
                    "name": "Mixed Event",
                    "optionalTimes": ["2024-03-15T10:00:00Z", null, "2024-03-16T10:00:00Z"],
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({ "name": "Empty Event", "optionalTimes": [null, null] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Events(filter: {optionalTimes: {_any: {_eq: "2024-03-15T10:00:00Z"}}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Events": [{ "name": "Mixed Event" }] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.read.datetime")]
fn query_inline_array_with_nillable_date_time() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Events {
                        name: String
                        scheduledTimes: [DateTime!]
                        optionalTimes: [DateTime]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Events".into(),
                doc_map: Some(json!({
                    "name": "Workshop",
                    "optionalTimes": ["2024-02-20T10:00:00Z", null, "2024-02-21T15:00:00Z"],
                })),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Events {
                                            name
                                            optionalTimes
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Events": [
                        {
                            "name": "Workshop",
                            "optionalTimes": ["2024-02-20T10:00:00Z", null, "2024-02-21T15:00:00Z"],
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
