//! Generated from the conformance scenarios of tests/integration/index/query_with_index_on_datetime_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.query.datetime")]
fn query_with_index_with_eq_filter_on_date_time_field_should_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        birthday: DateTime @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "birthday": "2000-07-23T03:00:00-00:00" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "birthday": "2001-08-23T03:00:00-00:00" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {birthday: {_eq: "2000-07-23T03:00:00-00:00"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Fred" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {birthday: {_eq: "2000-07-23T03:00:00-00:00"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.datetime")]
fn query_with_index_with_ge_filter_on_date_time_field_should_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        birthday: DateTime @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "birthday": "2000-07-23T03:00:00-00:00" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "birthday": "2001-08-23T03:00:00-00:00" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "birthday": "2001-01-01T00:00:00-00:00" })),
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
                            User(filter: {birthday: {_geq: "2001-01-01T00:00:00-00:00"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Keenan" }, { "name": "Andy" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {birthday: {_geq: "2001-01-01T00:00:00-00:00"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.datetime")]
fn query_with_index_with_gt_filter_on_date_time_field_should_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        birthday: DateTime @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "name": "Shahzad", "birthday": "2001-08-24T03:00:00-00:00" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "birthday": "2000-08-22T03:00:00-00:00" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "birthday": "2001-08-23T03:00:00-00:00" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "birthday": "2001-08-25T03:00:00-00:00" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {birthday: {_gt: "2001-08-23T03:00:00-00:00"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Shahzad" }, { "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {birthday: {_gt: "2001-08-23T03:00:00-00:00"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.datetime")]
fn query_with_index_with_le_filter_on_date_time_field_should_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        birthday: DateTime @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "birthday": "2000-07-23T03:00:00-00:00" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "birthday": "2001-08-23T03:00:00-00:00" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "birthday": "2001-01-01T00:00:00-00:00" })),
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
                            User(filter: {birthday: {_leq: "2001-01-01T00:00:00-00:00"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Fred" }, { "name": "Keenan" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {birthday: {_leq: "2001-01-01T00:00:00-00:00"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.datetime")]
fn query_with_index_with_lt_filter_on_date_time_field_should_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        birthday: DateTime @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "birthday": "2000-07-23T03:00:00-00:00" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "birthday": "2001-08-23T03:00:00-00:00" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {birthday: {_lt: "2001-01-01T00:00:00-00:00"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Fred" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {birthday: {_lt: "2001-01-01T00:00:00-00:00"}}) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.query.datetime")]
fn query_with_index_with_ne_filter_on_date_time_field_should_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        birthday: DateTime @index
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "birthday": "2000-07-23T03:00:00-00:00" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "birthday": "2001-08-23T03:00:00-00:00" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(filter: {birthday: {_neq: "2000-07-23T03:00:00-00:00"}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Andy" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
