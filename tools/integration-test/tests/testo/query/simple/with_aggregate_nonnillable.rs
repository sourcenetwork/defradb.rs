//! Generated from the conformance scenarios of tests/integration/query/simple/with_aggregate_nonnillable_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_average_on_non_nillable_float32() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { weight: Float32! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "weight": 60.0 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "weight": 80.0 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { AVG(Users: {field: weight}) }".into(),
                expected_data: json!({ "AVG": 70.0 }),
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
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_average_on_non_nillable_float64() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { rating: Float64! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "rating": 1.5 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "rating": 2.5 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { AVG(Users: {field: rating}) }".into(),
                expected_data: json!({ "AVG": 2.0 }),
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
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_average_on_non_nillable_int() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { score: Int! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 10 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 20 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 30 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { AVG(Users: {field: score}) }".into(),
                expected_data: json!({ "AVG": 20.0 }),
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
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_count_on_non_nillable_float32() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { weight: Float32! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "weight": 60.0 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "weight": 80.0 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { COUNT(Users: {}) }".into(),
                expected_data: json!({ "COUNT": 2 }),
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
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_count_on_non_nillable_float64() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { rating: Float64! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "rating": 1.5 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "rating": 2.5 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { COUNT(Users: {}) }".into(),
                expected_data: json!({ "COUNT": 2 }),
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
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_count_on_non_nillable_int() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { score: Int! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 10 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 20 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 30 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { COUNT(Users: {}) }".into(),
                expected_data: json!({ "COUNT": 3 }),
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
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_max_on_non_nillable_float32() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { weight: Float32! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "weight": 60.0 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "weight": 80.0 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { MAX(Users: {field: weight}) }".into(),
                expected_data: json!({ "MAX": 80.0 }),
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
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_max_on_non_nillable_float64() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { rating: Float64! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "rating": 1.5 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "rating": 3.5 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { MAX(Users: {field: rating}) }".into(),
                expected_data: json!({ "MAX": 3.5 }),
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
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_max_on_non_nillable_int() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { score: Int! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 10 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 20 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 30 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { MAX(Users: {field: score}) }".into(),
                expected_data: json!({ "MAX": 30.0 }),
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
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_min_on_non_nillable_float32() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { weight: Float32! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "weight": 60.0 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "weight": 80.0 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { MIN(Users: {field: weight}) }".into(),
                expected_data: json!({ "MIN": 60.0 }),
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
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_min_on_non_nillable_float64() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { rating: Float64! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "rating": 1.5 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "rating": 3.5 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { MIN(Users: {field: rating}) }".into(),
                expected_data: json!({ "MIN": 1.5 }),
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
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_min_on_non_nillable_int() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { score: Int! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 10 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 20 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 30 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { MIN(Users: {field: score}) }".into(),
                expected_data: json!({ "MIN": 10.0 }),
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
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_sum_on_non_nillable_float32() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { weight: Float32! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "weight": 60.0 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "weight": 80.0 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { SUM(Users: {field: weight}) }".into(),
                expected_data: json!({ "SUM": 140.0 }),
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
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_sum_on_non_nillable_float64() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { rating: Float64! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "rating": 1.5 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "rating": 2.5 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { SUM(Users: {field: rating}) }".into(),
                expected_data: json!({ "SUM": 4.0 }),
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
#[mantra_macros::req_test("go.query.simple.aggregate.non-nillable")]
fn query_simple_with_sum_on_non_nillable_int() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { score: Int! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 10 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 20 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "score": 30 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { SUM(Users: {field: score}) }".into(),
                expected_data: json!({ "SUM": 60 }),
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
