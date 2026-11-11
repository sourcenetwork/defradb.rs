//! Generated from the conformance scenarios of tests/integration/index/query_with_relation_sub_filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.relation.sub-filter")]
fn query_with_index_on_one_to_many_if_sub_filter_and_order_on_indexed_field_should_use_index_for_filter(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        devices: [Device]
                    }
                    type Device {
                        model: String @index
                        owner: User
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "Walkman", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "Discman", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "Someman", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "Jumpman", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "Galaxy", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "iPod", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_5", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User {
                                name
                                devices(filter: {model: {_like: "%man"}}, order: {model: ASC}, limit: 2) {
                                    model
                                }
                            }
                        }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "devices": [{ "model": "Discman" }, { "model": "Jumpman" }],
                            "name": "Chris",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User {
                                name
                                devices(filter: {model: {_like: "%man"}}, order: {model: ASC}, limit: 2) {
                                    model
                                }
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan_at(json!({ "indexFetches": 3 }), json!(["subType"]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-filter")]
fn query_with_index_on_one_to_many_if_sub_filter_on_indexed_field_should_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        devices: [Device]
                    }
                    type Device {
                        model: String @index
                        manufacturer: String
                        owner: User
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Sony", "model": "Walkman", "owner": m::r("doc0_0") }),
                ),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Sony", "model": "Discman", "owner": m::r("doc0_0") }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Aiwa", "model": "Walkman", "owner": m::r("doc0_0") }),
                ),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Apple", "model": "iPod", "owner": m::r("doc0_0") }),
                ),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User {
                                name
                                devices(filter: {model: {_eq: "Walkman"}}) {
                                    model
                                    manufacturer
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        {
                            "devices": [
                                { "manufacturer": "Aiwa", "model": "Walkman" },
                                { "manufacturer": "Sony", "model": "Walkman" },
                            ],
                            "name": "Chris",
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/User".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User {
                                name
                                devices(filter: {model: {_eq: "Walkman"}}) {
                                    model
                                    manufacturer
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 2 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-filter")]
fn query_with_index_on_one_to_many_if_sub_filter_on_non_indexed_field_should_not_use_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        devices: [Device]
                    }
                    type Device {
                        model: String @index
                        manufacturer: String
                        owner: User
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Sony", "model": "Walkman", "owner": m::r("doc0_0") }),
                ),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Sony", "model": "Discman", "owner": m::r("doc0_0") }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Aiwa", "model": "Walkman", "owner": m::r("doc0_0") }),
                ),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User {
                                name
                                devices(filter: {manufacturer: {_eq: "Sony"}}) {
                                    model
                                    manufacturer
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        {
                            "devices": [
                                { "manufacturer": "Sony", "model": "Discman" },
                                { "manufacturer": "Sony", "model": "Walkman" },
                            ],
                            "name": "Chris",
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/User".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User {
                                name
                                devices(filter: {manufacturer: {_eq: "Sony"}}) {
                                    model
                                    manufacturer
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 0 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-filter")]
fn query_with_index_on_one_to_many_with_order_on_parent_and_sub_filter_should_filter_both_with_indexes(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                        devices: [Device]
                    }
                    type Device {
                        model: String @index
                        manufacturer: String
                        owner: User
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Sony", "model": "Walkman", "owner": m::r("doc0_0") }),
                ),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Apple", "model": "iPod", "owner": m::r("doc0_0") }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Aiwa", "model": "Walkman", "owner": m::r("doc0_1") }),
                ),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Sony", "model": "Discman", "owner": m::r("doc0_1") }),
                ),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_eq: "Alice"}}) {
                                name
                                devices(filter: {model: {_eq: "Walkman"}}) {
                                    model
                                    manufacturer
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        {
                            "devices": [{ "manufacturer": "Sony", "model": "Walkman" }],
                            "name": "Alice",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {name: {_eq: "Alice"}}) {
                                name
                                devices(filter: {model: {_eq: "Walkman"}}) {
                                    model
                                    manufacturer
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 1 }),
                    json!(["root"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-filter")]
fn query_with_index_on_one_to_many_with_order_on_parent_and_sub_filter_should_filter_per_parent() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                        devices: [Device]
                    }
                    type Device {
                        model: String @index
                        manufacturer: String
                        owner: User
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Sony", "model": "Walkman", "owner": m::r("doc0_0") }),
                ),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Apple", "model": "iPod", "owner": m::r("doc0_0") }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Aiwa", "model": "Walkman", "owner": m::r("doc0_1") }),
                ),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Sony", "model": "Discman", "owner": m::r("doc0_1") }),
                ),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(order: {name: ASC}) {
                                name
                                devices(filter: {model: {_eq: "Walkman"}}) {
                                    model
                                    manufacturer
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        {
                            "devices": [{ "manufacturer": "Sony", "model": "Walkman" }],
                            "name": "Alice",
                        },
                        {
                            "devices": [{ "manufacturer": "Aiwa", "model": "Walkman" }],
                            "name": "Bob",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(order: {name: ASC}) {
                                name
                                devices(filter: {model: {_eq: "Walkman"}}) {
                                    model
                                    manufacturer
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 2 }),
                    json!(["root"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-filter")]
fn query_with_index_on_one_to_many_with_parent_filter_on_own_field_and_relation_and_sub_filter_should_combine_all_filters(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                        devices: [Device]
                    }
                    type Device {
                        model: String @index
                        manufacturer: String @index
                        owner: User
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "manufacturer": "Sony", "model": "Walkman", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "manufacturer": "Aiwa", "model": "Walkman", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "manufacturer": "Apple", "model": "iPod", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "manufacturer": "Sony", "model": "Walkman", "owner": m::r("doc0_1") })),
                common: Common { capture: captures([("doc1_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_eq: "Alice"}, devices: {model: {_eq: "Walkman"}}}) {
                                name
                                devices(filter: {manufacturer: {_eq: "Sony"}}) {
                                    model
                                    manufacturer
                                }
                            }
                        }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "devices": [{ "manufacturer": "Sony", "model": "Walkman" }],
                            "name": "Alice",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {name: {_eq: "Alice"}, devices: {model: {_eq: "Walkman"}}}) {
                                name
                                devices(filter: {manufacturer: {_eq: "Sony"}}) {
                                    model
                                    manufacturer
                                }
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan_at(json!({ "indexFetches": 5 }), json!(["subType"]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-filter")]
fn query_with_index_on_one_to_many_with_parent_filter_on_relation_and_sub_filter_on_different_indexed_field_should_use_both_indexes(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                        devices: [Device]
                    }
                    type Device {
                        model: String @index
                        manufacturer: String @index
                        owner: User
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "manufacturer": "Sony", "model": "Walkman", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "manufacturer": "Aiwa", "model": "Walkman", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "manufacturer": "Toshiba", "model": "Walkman", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "manufacturer": "Apple", "model": "iPod", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "manufacturer": "Google", "model": "Pixel", "owner": m::r("doc0_1") })),
                common: Common { capture: captures([("doc1_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {devices: {model: {_eq: "Walkman"}}}) {
                                name
                                devices(filter: {manufacturer: {_eq: "Sony"}}) {
                                    model
                                    manufacturer
                                }
                            }
                        }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "devices": [{ "manufacturer": "Sony", "model": "Walkman" }],
                            "name": "Alice",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {devices: {model: {_eq: "Walkman"}}}) {
                                name
                                devices(filter: {manufacturer: {_eq: "Sony"}}) {
                                    model
                                    manufacturer
                                }
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan_at(json!({ "indexFetches": 4 }), json!(["subType"]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-filter")]
fn query_with_index_on_one_to_many_with_parent_filter_on_relation_and_sub_filter_on_non_indexed_field_should_use_parent_index(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                        devices: [Device]
                    }
                    type Device {
                        model: String @index
                        manufacturer: String
                        owner: User
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Sony", "model": "Walkman", "owner": m::r("doc0_0") }),
                ),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Aiwa", "model": "Walkman", "owner": m::r("doc0_0") }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Google", "model": "Pixel", "owner": m::r("doc0_1") }),
                ),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {devices: {model: {_eq: "Walkman"}}}) {
                                name
                                devices(filter: {manufacturer: {_eq: "Sony"}}) {
                                    model
                                    manufacturer
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        {
                            "devices": [{ "manufacturer": "Sony", "model": "Walkman" }],
                            "name": "Alice",
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {devices: {model: {_eq: "Walkman"}}}) {
                                name
                                devices(filter: {manufacturer: {_eq: "Sony"}}) {
                                    model
                                    manufacturer
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 2 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-filter")]
fn query_with_index_on_one_to_many_with_same_filter_on_parent_and_sub_type_should_filter_both_with_indexes(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                        devices: [Device]
                    }
                    type Device {
                        model: String @index
                        owner: User
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "Walkman", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "iPod", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "Galaxy", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "Pixel", "owner": m::r("doc0_1") })),
                common: Common { capture: captures([("doc1_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {devices: {model: {_eq: "Walkman"}}}) {
                                name
                                devices(filter: {model: {_eq: "Galaxy"}}) {
                                    model
                                }
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [{ "devices": [{ "model": "Galaxy" }], "name": "Alice" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {devices: {model: {_eq: "Walkman"}}}) {
                                name
                                devices(filter: {model: {_eq: "Galaxy"}}) {
                                    model
                                }
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan_at(json!({ "indexFetches": 2 }), json!(["subType"]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.sub-filter")]
fn query_with_index_on_one_to_many_with_same_filter_value_on_parent_and_sub_type_should_return_matching_docs(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                        devices: [Device]
                    }
                    type Device {
                        model: String @index
                        owner: User
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "Walkman", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "iPod", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "Pixel", "owner": m::r("doc0_1") })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {devices: {model: {_eq: "Walkman"}}}) {
                                name
                                devices(filter: {model: {_eq: "Walkman"}}) {
                                    model
                                }
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [{ "devices": [{ "model": "Walkman" }], "name": "Alice" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {devices: {model: {_eq: "Walkman"}}}) {
                                name
                                devices(filter: {model: {_eq: "Walkman"}}) {
                                    model
                                }
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan_at(json!({ "indexFetches": 2 }), json!(["subType"]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
