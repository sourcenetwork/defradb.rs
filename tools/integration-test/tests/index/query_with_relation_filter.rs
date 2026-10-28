//! Generated from the conformance scenarios of tests/integration/index/query_with_relation_filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.relation.order-orphans-exhaustive")]
fn query_with_index_exhaustive_order_by_relation_with_scalar_filter_descending_should_filter_orphans(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        rating: Int @index
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "rating": 5, "title": "Book1" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "book": m::r("doc0_0"), "name": "LinkedPublisher" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "IncludedOrphan" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "ExcludedOrphan" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @exhaustive {
                                        Publisher(
                                            filter: {name: {_neq: "ExcludedOrphan"}},
                                            order: {book: {rating: DESC}}
                                        ) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Publisher": [{ "name": "LinkedPublisher" }, { "name": "IncludedOrphan" }],
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
#[mantra_macros::req_test("go.index.relation.order-orphans-exhaustive")]
fn query_with_index_exhaustive_order_by_relation_with_scalar_filter_should_filter_orphans() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        rating: Int @index
                        publisher: Publisher
                    }
                    type Publisher {
                        name: String
                        book: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "rating": 5, "title": "Book1" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "book": m::r("doc0_0"), "name": "LinkedPublisher" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "IncludedOrphan" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Publisher".into(),
                doc_map: Some(json!({ "name": "ExcludedOrphan" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @exhaustive {
                                        Publisher(
                                            filter: {name: {_neq: "ExcludedOrphan"}},
                                            order: {book: {rating: ASC}}
                                        ) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Publisher": [{ "name": "IncludedOrphan" }, { "name": "LinkedPublisher" }],
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
#[mantra_macros::req_test("go.index.relation.one-to-many-filter")]
fn query_with_index_on_many_to_one_if_filter_on_indexed_field_should_filter_with_explain() {
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
                        model: String 
                        year: Int @index
                        owner: User
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
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
                    json!({ "_ownerID": m::r("doc0_0"), "model": "iPhone Xs", "year": 2022 }),
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
                    json!({ "_ownerID": m::r("doc0_0"), "model": "MacBook Pro", "year": 2020 }),
                ),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({
                    "_ownerID": m::r("doc0_3"),
                    "model": "Samsung Galaxy S20",
                    "year": 2022,
                })),
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
                    json!({ "_ownerID": m::r("doc0_3"), "model": "Lenovo ThinkPad", "year": 2020 }),
                ),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_4"), "model": "Google Pixel 5", "year": 2022 }),
                ),
                common: Common {
                    capture: captures([("doc1_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_4"), "model": "Asus Vivobook", "year": 2022 }),
                ),
                common: Common {
                    capture: captures([("doc1_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_4"), "model": "Commodore 64", "year": 1982 }),
                ),
                common: Common {
                    capture: captures([("doc1_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_5"), "model": "iPhone 12s", "year": 2018 }),
                ),
                common: Common {
                    capture: captures([("doc1_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_5"), "model": "MacBook Pro", "year": 2023 }),
                ),
                common: Common {
                    capture: captures([("doc1_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_5"), "model": "iPad Pro", "year": 2020 }),
                ),
                common: Common {
                    capture: captures([("doc1_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_5"), "model": "Playstation 5", "year": 2021 }),
                ),
                common: Common {
                    capture: captures([("doc1_10", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_5"), "model": "Nokia 7610", "year": 2003 }),
                ),
                common: Common {
                    capture: captures([("doc1_11", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_6"), "model": "Xiaomi Phone", "year": 2022 }),
                ),
                common: Common {
                    capture: captures([("doc1_12", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_6"), "model": "Alienware x16", "year": 2018 }),
                ),
                common: Common {
                    capture: captures([("doc1_13", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_7"), "model": "iPhone 10", "year": 2021 }),
                ),
                common: Common {
                    capture: captures([("doc1_14", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_7"), "model": "Acer Aspire 5", "year": 2020 }),
                ),
                common: Common {
                    capture: captures([("doc1_15", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_7"), "model": "HyperX Headset", "year": 2014 }),
                ),
                common: Common {
                    capture: captures([("doc1_16", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_7"), "model": "Playstation 5", "year": 2021 }),
                ),
                common: Common {
                    capture: captures([("doc1_17", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_8"), "model": "iPhone 13", "year": 2022 }),
                ),
                common: Common {
                    capture: captures([("doc1_18", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_8"), "model": "MacBook Pro", "year": 2017 }),
                ),
                common: Common {
                    capture: captures([("doc1_19", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_8"), "model": "iPad Mini", "year": 2015 }),
                ),
                common: Common {
                    capture: captures([("doc1_20", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "_ownerID": m::r("doc0_9"), "model": "Walkman", "year": 2000 }),
                ),
                common: Common {
                    capture: captures([("doc1_21", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            Device(filter: {
                                year: {_eq: 2021}
                            }) {
                                model
                                owner {
                                    name
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Device": [
                        { "model": "iPhone 10", "owner": { "name": "Addo" } },
                        { "model": "Playstation 5", "owner": { "name": "Islam" } },
                        { "model": "Playstation 5", "owner": { "name": "Addo" } },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Device".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Device(filter: {
                                year: {_eq: 2021}
                            }) {
                                model
                                owner {
                                    name
                                }
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 3 }),
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
#[mantra_macros::req_test("go.index.relation.one-to-many-filter")]
fn query_with_index_on_many_to_one_if_filter_on_indexed_relation_should_filter_with_explain() {
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
                        model: String
                        owner: User @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_0"), "model": "iPhone Xs" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_0"), "model": "MacBook Pro" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_3"), "model": "Samsung Galaxy S20" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_3"), "model": "Lenovo ThinkPad" })),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_4"), "model": "Google Pixel 5" })),
                common: Common {
                    capture: captures([("doc1_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_4"), "model": "Asus Vivobook" })),
                common: Common {
                    capture: captures([("doc1_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_4"), "model": "Commodore 64" })),
                common: Common {
                    capture: captures([("doc1_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_5"), "model": "iPhone 12s" })),
                common: Common {
                    capture: captures([("doc1_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_5"), "model": "MacBook Pro" })),
                common: Common {
                    capture: captures([("doc1_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_5"), "model": "iPad Pro" })),
                common: Common {
                    capture: captures([("doc1_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_5"), "model": "Playstation 5" })),
                common: Common {
                    capture: captures([("doc1_10", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_5"), "model": "Nokia 7610" })),
                common: Common {
                    capture: captures([("doc1_11", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_6"), "model": "Xiaomi Phone" })),
                common: Common {
                    capture: captures([("doc1_12", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_6"), "model": "Alienware x16" })),
                common: Common {
                    capture: captures([("doc1_13", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_7"), "model": "iPhone 10" })),
                common: Common {
                    capture: captures([("doc1_14", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_7"), "model": "Acer Aspire 5" })),
                common: Common {
                    capture: captures([("doc1_15", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_7"), "model": "HyperX Headset" })),
                common: Common {
                    capture: captures([("doc1_16", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_7"), "model": "Playstation 5" })),
                common: Common {
                    capture: captures([("doc1_17", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_8"), "model": "iPhone 13" })),
                common: Common {
                    capture: captures([("doc1_18", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_8"), "model": "MacBook Pro" })),
                common: Common {
                    capture: captures([("doc1_19", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_8"), "model": "iPad Mini" })),
                common: Common {
                    capture: captures([("doc1_20", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_9"), "model": "Walkman" })),
                common: Common {
                    capture: captures([("doc1_21", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            Device(filter: {
                                owner: {name: {_eq: "Keenan"}}
                            }) {
                                model
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "Device": [
                        { "model": "MacBook Pro" },
                        { "model": "iPad Mini" },
                        { "model": "iPhone 13" },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Device".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Device(filter: {
                                owner: {name: {_eq: "Keenan"}}
                            }) {
                                model
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 3 }),
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
#[mantra_macros::req_test("go.index.relation.one-to-many-filter")]
fn query_with_index_on_many_to_one_multiple_via_one_to_many() {
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
                        model: String
                        owner: User @index
                        manufacturer: Manufacturer @index
                    }

                    type Manufacturer {
                        name: String
                        devices: [Device]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Manufacturer".into(),
                doc_map: Some(json!({ "name": "Apple" })),
                common: Common {
                    capture: captures([("doc2_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({
                    "manufacturer": m::r("doc2_0"),
                    "model": "MacBook Pro",
                    "owner": m::r("doc0_0"),
                })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            devices {
                                                _ownerID
                                                _manufacturerID
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        {
                            "devices": [
                                {
                                    "_manufacturerID": m::r("doc2_0"),
                                    "_ownerID": m::r("doc0_0"),
                                },
                            ],
                        },
                    ],
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
#[mantra_macros::req_test("go.index.relation.one-to-many-filter")]
fn query_with_index_on_one_to_many_if_filter_on_indexed_primary_doc_and_sub_filter_should_filter() {
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
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo" })),
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
                doc_map: Some(json!({
                    "manufacturer": "The Proclaimers",
                    "model": "Walkman",
                    "owner": m::r("doc0_0"),
                })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({
                    "manufacturer": "Braveworld Productions",
                    "model": "Running Man",
                    "owner": m::r("doc0_0"),
                })),
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
                                        User(filter: {
                                            devices: {model: {_eq: "Walkman"}}
                                        }) {
                                            name
                                            devices(filter: {manufacturer: {_neq: "Sony"}}) {
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
                                {
                                    "manufacturer": "Braveworld Productions",
                                    "model": "Running Man",
                                },
                                { "manufacturer": "The Proclaimers", "model": "Walkman" },
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
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.one-to-many-filter")]
fn query_with_index_on_one_to_many_if_filter_on_indexed_primary_doc_should_filter() {
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
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo" })),
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
                doc_map: Some(json!({
                    "manufacturer": "The Proclaimers",
                    "model": "Walkman",
                    "owner": m::r("doc0_0"),
                })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({
                    "manufacturer": "Braveworld Productions",
                    "model": "Running Man",
                    "owner": m::r("doc0_0"),
                })),
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
                                        User(filter: {
                                            devices: {model: {_eq: "Walkman"}}
                                        }) {
                                            name
                                            devices {
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
                                {
                                    "manufacturer": "Braveworld Productions",
                                    "model": "Running Man",
                                },
                                { "manufacturer": "The Proclaimers", "model": "Walkman" },
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
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.one-to-many-filter")]
fn query_with_index_on_one_to_many_if_filter_on_indexed_relation_should_filter_with_explain() {
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
                doc_map: Some(json!({
                    "manufacturer": "The Proclaimers",
                    "model": "Walkman",
                    "owner": m::r("doc0_0"),
                })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({
                    "manufacturer": "Braveworld Productions",
                    "model": "Running Man",
                    "owner": m::r("doc0_0"),
                })),
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
                            User(filter: {
                                devices: {model: {_eq: "Walkman"}}
                            }) {
                                name
                                devices {
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
                                {
                                    "manufacturer": "Braveworld Productions",
                                    "model": "Running Man",
                                },
                                { "manufacturer": "The Proclaimers", "model": "Walkman" },
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
                            User(filter: {
                                devices: {model: {_eq: "Walkman"}}
                            }) {
                                name
                                devices {
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
#[mantra_macros::req_test("go.index.relation.one-to-many-filter")]
fn query_with_index_on_one_to_many_if_indexed_relation_is_nil_eq_nil_filter_should_use_index() {
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
                        model: String 
                        manufacturer: String
                        owner: User @index
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
                doc_map: Some(json!({ "manufacturer": "Sony", "model": "Walkman", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "manufacturer": "Apple", "model": "iPhone", "owner": m::r("doc0_0") })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "Running Man", "manufacturer": "Braveworld Productions" })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "model": "PlayStation 5", "manufacturer": "Sony" })),
                common: Common { capture: captures([("doc1_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            Device(filter: {
                                _ownerID: {_eq: null}
                            }) {
                                model
                            }
                        }
                "#.into(),
                expected_data: json!({ "Device": [{ "model": "Running Man" }, { "model": "PlayStation 5" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Device(filter: {
                                _ownerID: {_eq: null}
                            }) {
                                model
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan_at(json!({ "indexFetches": 2 }), json!(["root"]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.one-to-many-filter")]
fn query_with_index_on_one_to_many_if_indexed_relation_is_nil_ne_nil_filter_should_use_index() {
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
                        model: String 
                        manufacturer: String
                        owner: User @index
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
                    json!({ "manufacturer": "Apple", "model": "iPhone", "owner": m::r("doc0_0") }),
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
                    json!({ "model": "Running Man", "manufacturer": "Braveworld Productions" }),
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
                doc_map: Some(json!({ "model": "PlayStation 5", "manufacturer": "Sony" })),
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
                            Device(filter: {
                                _ownerID: {_neq: null}
                            }) {
                                model
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "Device": [{ "model": "Walkman" }, { "model": "iPhone" }] }),
                common: Common {
                    unordered_documents: vec!["/data/Device".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            Device(filter: {
                                _ownerID: {_neq: null}
                            }) {
                                model
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 4 }),
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
#[mantra_macros::req_test("go.index.relation.one-to-many-filter")]
fn query_with_index_on_one_to_many_relation_if_filter_on_indexed_relation_should_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        age: Int
                        devices: [Device] 
                    } 

                    type Device {
                        model: String @index
                        owner: User
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 20, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_0"), "model": "iPhone Xs" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_0"), "model": "MacBook Pro" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 23, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 44, "name": "Roy" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 28, "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_3"), "model": "Samsung Galaxy S20" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_3"), "model": "Lenovo ThinkPad" })),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_4"), "model": "Google Pixel 5" })),
                common: Common {
                    capture: captures([("doc1_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_4"), "model": "Asus Vivobook" })),
                common: Common {
                    capture: captures([("doc1_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_4"), "model": "Commodore 64" })),
                common: Common {
                    capture: captures([("doc1_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 32, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_5"), "model": "iPhone 12s" })),
                common: Common {
                    capture: captures([("doc1_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_5"), "model": "MacBook Pro" })),
                common: Common {
                    capture: captures([("doc1_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_5"), "model": "iPad Pro" })),
                common: Common {
                    capture: captures([("doc1_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_5"), "model": "Playstation 5" })),
                common: Common {
                    capture: captures([("doc1_10", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_5"), "model": "Nokia 7610" })),
                common: Common {
                    capture: captures([("doc1_11", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 33, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_6"), "model": "Xiaomi Phone" })),
                common: Common {
                    capture: captures([("doc1_12", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_6"), "model": "Alienware x16" })),
                common: Common {
                    capture: captures([("doc1_13", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 42, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_7"), "model": "iPhone 10" })),
                common: Common {
                    capture: captures([("doc1_14", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_7"), "model": "Acer Aspire 5" })),
                common: Common {
                    capture: captures([("doc1_15", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_7"), "model": "HyperX Headset" })),
                common: Common {
                    capture: captures([("doc1_16", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_7"), "model": "Playstation 5" })),
                common: Common {
                    capture: captures([("doc1_17", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 48, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_8"), "model": "iPhone 13" })),
                common: Common {
                    capture: captures([("doc1_18", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_8"), "model": "MacBook Pro" })),
                common: Common {
                    capture: captures([("doc1_19", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_8"), "model": "iPad Mini" })),
                common: Common {
                    capture: captures([("doc1_20", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 55, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(json!({ "_ownerID": m::r("doc0_9"), "model": "Walkman" })),
                common: Common {
                    capture: captures([("doc1_21", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {
                                devices: {model: {_eq: "MacBook Pro"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "User": [{ "name": "Shahzad" }, { "name": "Keenan" }, { "name": "Islam" }],
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
                            User(filter: {
                                devices: {model: {_eq: "MacBook Pro"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 3 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {
                                devices: {model: {_eq: "iPhone 10"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Addo" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {
                                devices: {model: {_eq: "iPhone 10"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 1 }),
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
#[mantra_macros::req_test("go.index.relation.one-to-one-filter")]
fn query_with_index_on_one_to_one_if_filter_on_indexed_relation_should_filter() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        address: Address
                    }

                    type Address {
                        user: User @primary
                        city: String @index
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_0"), "city": "Montreal" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bruno" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_1"), "city": "New York" })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Roy" })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_2"), "city": "Los Angeles" })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred" })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_3"), "city": "Montreal" })),
                common: Common { capture: captures([("doc1_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_4"), "city": "Montreal" })),
                common: Common { capture: captures([("doc1_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam" })),
                common: Common { capture: captures([("doc0_5", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_5"), "city": "Munich" })),
                common: Common { capture: captures([("doc1_5", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy" })),
                common: Common { capture: captures([("doc0_6", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_6"), "city": "London" })),
                common: Common { capture: captures([("doc1_6", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Addo" })),
                common: Common { capture: captures([("doc0_7", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_7"), "city": "Ottawa" })),
                common: Common { capture: captures([("doc1_7", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan" })),
                common: Common { capture: captures([("doc0_8", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_8"), "city": "San Francisco" })),
                common: Common { capture: captures([("doc1_8", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris" })),
                common: Common { capture: captures([("doc0_9", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_9"), "city": "Toronto" })),
                common: Common { capture: captures([("doc1_9", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {
                                address: {city: {_eq: "Munich"}}
                            }) {
                                name
                                address {
                                    city
                                }
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [{ "address": { "city": "Munich" }, "name": "Islam" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {
                                address: {city: {_eq: "Munich"}}
                            }) {
                                name
                                address {
                                    city
                                }
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan_at(json!({ "indexFetches": 1 }), json!(["subType"]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.one-to-one-filter")]
fn query_with_index_on_one_to_one_primary_relation_if_filter_on_indexed_field_of_relation_and_relation_should_filter(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                        address: Address @primary @index(unique: true)
                    }

                    type Address {
                        user: User
                        city: String @index
                        street: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Montreal", "street": "Queen Mary Rd" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "_addressID": m::r("doc1_0"), "age": 20, "name": "Shahzad" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "New York", "street": "5th Ave" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_1"), "age": 23, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Los Angeles", "street": "Hollywood Blvd" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_2"), "age": 44, "name": "Roy" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Montreal", "street": "Park Ave" })),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_3"), "age": 28, "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Montreal", "street": "Notre-Dame St W" })),
                common: Common {
                    capture: captures([("doc1_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_4"), "age": 30, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Munich", "street": "Leopold Str" })),
                common: Common {
                    capture: captures([("doc1_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_5"), "age": 32, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "London", "street": "Baker St" })),
                common: Common {
                    capture: captures([("doc1_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_6"), "age": 33, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Ottawa", "street": "Bank St" })),
                common: Common {
                    capture: captures([("doc1_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_7"), "age": 42, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "San Francisco", "street": "Market St" })),
                common: Common {
                    capture: captures([("doc1_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_8"), "age": 48, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Toronto", "street": "Yonge St" })),
                common: Common {
                    capture: captures([("doc1_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_9"), "age": 55, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {
                                address: {city: {_eq: "London"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Andy" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {
                                address: {city: {_eq: "London"}}
                            }) {
                                name
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
            Request {
                request: r#"
                    query {
                            User(filter: {
                                address: {city: {_eq: "Montreal"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "User": [{ "name": "Shahzad" }, { "name": "John" }, { "name": "Fred" }],
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
                            User(filter: {
                                address: {city: {_eq: "Montreal"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 3 }),
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
#[mantra_macros::req_test("go.index.relation.one-to-one-filter")]
fn query_with_index_on_one_to_one_primary_relation_if_filter_on_indexed_field_of_relation_should_filter(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        age: Int
                        address: Address @primary 
                    } 

                    type Address {
                        user: User
                        city: String @index
                        street: String 
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Montreal", "street": "Queen Mary Rd" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "_addressID": m::r("doc1_0"), "age": 20, "name": "Shahzad" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "New York", "street": "5th Ave" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_1"), "age": 23, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Los Angeles", "street": "Hollywood Blvd" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_2"), "age": 44, "name": "Roy" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Montreal", "street": "Park Ave" })),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_3"), "age": 28, "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Montreal", "street": "Notre-Dame St W" })),
                common: Common {
                    capture: captures([("doc1_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_4"), "age": 30, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Munich", "street": "Leopold Str" })),
                common: Common {
                    capture: captures([("doc1_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_5"), "age": 32, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "London", "street": "Baker St" })),
                common: Common {
                    capture: captures([("doc1_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_6"), "age": 33, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Ottawa", "street": "Bank St" })),
                common: Common {
                    capture: captures([("doc1_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_7"), "age": 42, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "San Francisco", "street": "Market St" })),
                common: Common {
                    capture: captures([("doc1_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_8"), "age": 48, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Toronto", "street": "Yonge St" })),
                common: Common {
                    capture: captures([("doc1_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_9"), "age": 55, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {
                                address: {city: {_eq: "London"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Andy" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {
                                address: {city: {_eq: "London"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "fieldFetches": 3, "indexFetches": 1 }),
                    json!(["root"])
                )),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {
                                address: {city: {_eq: "Montreal"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "User": [{ "name": "Shahzad" }, { "name": "John" }, { "name": "Fred" }],
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
                            User(filter: {
                                address: {city: {_eq: "Montreal"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "fieldFetches": 9, "indexFetches": 3 }),
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
#[mantra_macros::req_test("go.index.relation.one-to-one-filter")]
fn query_with_index_on_one_to_one_primary_relation_if_filter_on_indexed_relation_while_indexed_foreign_field_should_filter(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                        address: Address @primary @index(unique: true)
                    }

                    type Address {
                        user: User
                        city: String @index
                        street: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Montreal", "street": "Queen Mary Rd" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "_addressID": m::r("doc1_0"), "age": 20, "name": "Shahzad" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "New York", "street": "5th Ave" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_1"), "age": 23, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Los Angeles", "street": "Hollywood Blvd" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_2"), "age": 44, "name": "Roy" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Montreal", "street": "Park Ave" })),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_3"), "age": 28, "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Montreal", "street": "Notre-Dame St W" })),
                common: Common {
                    capture: captures([("doc1_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_4"), "age": 30, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Munich", "street": "Leopold Str" })),
                common: Common {
                    capture: captures([("doc1_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_5"), "age": 32, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "London", "street": "Baker St" })),
                common: Common {
                    capture: captures([("doc1_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_6"), "age": 33, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Ottawa", "street": "Bank St" })),
                common: Common {
                    capture: captures([("doc1_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_7"), "age": 42, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "San Francisco", "street": "Market St" })),
                common: Common {
                    capture: captures([("doc1_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_8"), "age": 48, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "city": "Toronto", "street": "Yonge St" })),
                common: Common {
                    capture: captures([("doc1_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_9"), "age": 55, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {
                                address: {city: {_eq: "London"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Andy" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {
                                address: {city: {_eq: "London"}}
                            }) {
                                name
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
#[mantra_macros::req_test("go.index.relation.one-to-one-filter")]
fn query_with_index_on_one_to_ones_secondary_relation_if_filter_on_indexed_relation_should_filter()
{
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        age: Int
                        address: Address
                    } 

                    type Address {
                        user: User @primary
                        city: String @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 20, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_0"), "city": "Montreal" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 23, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_1"), "city": "New York" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 44, "name": "Roy" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_2"), "city": "Los Angeles" })),
                common: Common {
                    capture: captures([("doc1_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 28, "name": "Fred" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_3"), "city": "Montreal" })),
                common: Common {
                    capture: captures([("doc1_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_4"), "city": "Montreal" })),
                common: Common {
                    capture: captures([("doc1_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 32, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_5"), "city": "Munich" })),
                common: Common {
                    capture: captures([("doc1_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 33, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_6"), "city": "London" })),
                common: Common {
                    capture: captures([("doc1_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 42, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_7"), "city": "Ottawa" })),
                common: Common {
                    capture: captures([("doc1_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 48, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_8"), "city": "San Francisco" })),
                common: Common {
                    capture: captures([("doc1_8", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 55, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "_userID": m::r("doc0_9"), "city": "Toronto" })),
                common: Common {
                    capture: captures([("doc1_9", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {
                                address: {city: {_eq: "Munich"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Islam" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {
                                address: {city: {_eq: "Munich"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 1 }),
                    json!(["subType"])
                )),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {
                                address: {city: {_eq: "Montreal"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!({
                    "User": [{ "name": "Shahzad" }, { "name": "John" }, { "name": "Fred" }],
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
                            User(filter: {
                                address: {city: {_eq: "Montreal"}}
                            }) {
                                name
                            }
                        }
                "#
                .into(),
                expected_data: json!(m::explain_scan_at(
                    json!({ "indexFetches": 3 }),
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
#[mantra_macros::req_test("go.index.relation.combined-filter")]
fn query_with_index_with_multiple_scalars_and_relation_filter_should_apply_all_as_and() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String @index
                        published: [Book]
                    }

                    type Book {
                        title: String
                        rating: Float
                        genre: String
                        author: Author
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "author": m::r("doc0_0"),
                    "genre": "drama",
                    "rating": 4.9,
                    "title": "Painted House",
                })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "author": m::r("doc0_0"),
                    "genre": "thriller",
                    "rating": 4.0,
                    "title": "A Time to Kill",
                })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "author": m::r("doc0_0"),
                    "genre": "thriller",
                    "rating": 4.5,
                    "title": "The Firm",
                })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({
                    "author": m::r("doc0_1"),
                    "genre": "fantasy",
                    "rating": 4.8,
                    "title": "Theif Lord",
                })),
                common: Common { capture: captures([("doc1_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book(filter: {genre: {_eq: "thriller"}, rating: {_gt: 4.0}, author: {name: {_eq: "John Grisham"}}}) {
                                            title
                                            rating
                                            genre
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Book": [{ "genre": "thriller", "rating": 4.5, "title": "The Firm" }] }),
                common: Common { unordered_documents: vec!["/data/Book".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.combined-filter")]
fn query_with_index_with_scalar_and_relation_filter_at_top_level_should_apply_both_as_and() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String @index
                        published: [Book]
                    }

                    type Book {
                        title: String
                        rating: Float
                        author: Author
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham" })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke" })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 4.9, "title": "Painted House" })),
                common: Common { capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_0"), "rating": 4.0, "title": "A Time to Kill" })),
                common: Common { capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_1"), "rating": 4.8, "title": "Theif Lord" })),
                common: Common { capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "author": m::r("doc0_1"), "rating": 4.4, "title": "Inkheart" })),
                common: Common { capture: captures([("doc1_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book(filter: {rating: {_gt: 4.5}, author: {name: {_eq: "John Grisham"}}}) {
                                            title
                                            rating
                                            author {
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Book": [
                        {
                            "author": { "name": "John Grisham" },
                            "rating": 4.9,
                            "title": "Painted House",
                        },
                    ],
                }),
                common: Common { unordered_documents: vec!["/data/Book".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book(filter: {_and: [{rating: {_gt: 4.5}}, {author: {name: {_eq: "John Grisham"}}}]}) {
                                            title
                                            rating
                                            author {
                                                name
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Book": [
                        {
                            "author": { "name": "John Grisham" },
                            "rating": 4.9,
                            "title": "Painted House",
                        },
                    ],
                }),
                common: Common { unordered_documents: vec!["/data/Book".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.relation.one-to-one-filter")]
fn query_with_unique_index_with_filter_on_child_indexed_field_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index(unique: true)
                        devices: [Device]
                    }

                    type Device {
                        trusted: Boolean
                        owner: User
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
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
                                        Device(filter: {owner: {name: {_eq: "John"}}}) {
                                            trusted
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Device": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
