//! Pins that a scalar filter is applied once, at the scan, under an order and a limit.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.explain.default.filter")]
fn explain_filter_with_order_and_limit_is_applied_only_at_the_scan() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: simple) {
                        Book(filter: {title: {_eq: "1984"}}, limit: 3, order: {title: ASC}) {
                            title
                        }
                    }
                "#
                .into(),
                expected_data: json!(m::all(json!([
                    m::explain_pattern(json!({
                        "explain": {
                            "operationNode": [{
                                "selectTopNode": {
                                    "limitNode": {
                                        "orderNode": { "selectNode": { "scanNode": {} } },
                                    },
                                },
                            }],
                        },
                    })),
                    m::explain_target("limitNode", false, 0, json!({
                        "limit": 3,
                        "offset": 0,
                    })),
                    m::explain_target("orderNode", false, 0, json!({
                        "orderings": [{ "direction": "ASC", "fields": ["title"] }],
                    })),
                    m::explain_target("selectNode", false, 0, json!({
                        "docID": null,
                        "filter": null,
                    })),
                    m::explain_target("scanNode", false, 0, json!({
                        "collectionID": "bafyreignnfga3vinwqt2awokqzfmovoggde2bug3q4xcq4ys6yxhwbzn3m",
                        "collectionName": "Book",
                        "filter": { "title": { "_eq": "1984" } },
                        "prefixes": ["/1"],
                    })),
                ]))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.explain.default.filter")]
fn filter_with_order_and_limit_returns_only_matching_documents() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        title: String
                        year: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({"title": "1984", "year": 1949})),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({"title": "Animal Farm", "year": 1945})),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({"title": "1984", "year": 1950})),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Book(filter: {title: {_eq: "1984"}}, limit: 3, order: {year: ASC}) {
                            title
                            year
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Book": [
                        {"title": "1984", "year": 1949},
                        {"title": "1984", "year": 1950},
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
