//! Generated from the conformance scenarios of tests/integration/query/inline_array/with_nonnillable_scalars_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.inline_array.non-nillable.scalar-required")]
fn query_non_nillable_blob_not_empty() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String, avatar: Blob! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "avatar": "ff0011" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { name, avatar } }".into(),
                expected_data: json!({ "Users": [{ "avatar": "ff0011", "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.non-nillable.scalar-required")]
fn query_non_nillable_blob_null() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String, avatar: Blob! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "avatar": null })),
                expected_error: Some(
                    "null value provided for non-nillable field. Name: avatar".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.non-nillable.scalar-required")]
fn query_non_nillable_blob_omitted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String, avatar: Blob! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                expected_error: Some(
                    "value not provided for non-nillable field. Name: avatar".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.non-nillable.scalar-required")]
fn query_non_nillable_date_time_not_empty() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String, createdAt: DateTime! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "createdAt": "2024-01-01T00:00:00Z" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { name } }".into(),
                expected_data: json!({ "Users": [{ "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.non-nillable.scalar-required")]
fn query_non_nillable_date_time_null() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String, createdAt: DateTime! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "createdAt": null })),
                expected_error: Some(
                    "null value provided for non-nillable field. Name: createdAt".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.non-nillable.scalar-required")]
fn query_non_nillable_date_time_omitted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String, createdAt: DateTime! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                expected_error: Some(
                    "value not provided for non-nillable field. Name: createdAt".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.non-nillable.scalar-required")]
fn query_non_nillable_json_not_empty() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String, metadata: JSON! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "metadata": { "role": "admin" } })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { name, metadata } }".into(),
                expected_data: json!({ "Users": [{ "metadata": { "role": "admin" }, "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.non-nillable.scalar-required")]
fn query_non_nillable_json_null() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String, metadata: JSON! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "metadata": null })),
                expected_error: Some(
                    "null value provided for non-nillable field. Name: metadata".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.non-nillable.scalar-required")]
fn query_non_nillable_json_omitted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String, metadata: JSON! }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                expected_error: Some(
                    "value not provided for non-nillable field. Name: metadata".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_booleans_not_empty() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { flags: [Boolean!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "flags": [true, false, true] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { flags } }".into(),
                expected_data: json!({ "Users": [{ "flags": [true, false, true] }] }),
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
fn query_inline_array_with_non_nillable_booleans_null_element() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { flags: [Boolean!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "flags": [true, null, false] })),
                expected_error: Some("null".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_booleans_null() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { flags: [Boolean!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "flags": null })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { flags } }".into(),
                expected_data: json!({ "Users": [{ "flags": null }] }),
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
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_booleans_omitted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { flags: [Boolean!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({})),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { flags } }".into(),
                expected_data: json!({ "Users": [{ "flags": null }] }),
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
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_float32s_not_empty() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { weights: [Float32!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "weights": [1.1, 2.2, 3.3] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { weights } }".into(),
                expected_data: json!({ "Users": [{ "weights": [1.1, 2.2, 3.3] }] }),
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
fn query_inline_array_with_non_nillable_float32s_null_element() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { weights: [Float32!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "weights": [1.1, null, 3.3] })),
                expected_error: Some("null value provided for non-nillable field".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_float32s_null() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { weights: [Float32!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "weights": null })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { weights } }".into(),
                expected_data: json!({ "Users": [{ "weights": null }] }),
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
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_float32s_omitted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { weights: [Float32!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({})),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { weights } }".into(),
                expected_data: json!({ "Users": [{ "weights": null }] }),
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
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_float64s_not_empty() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { ratings: [Float64!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "ratings": [1.1, 2.2, 3.3] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { ratings } }".into(),
                expected_data: json!({ "Users": [{ "ratings": [1.1, 2.2, 3.3] }] }),
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
fn query_inline_array_with_non_nillable_float64s_null_element() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { ratings: [Float64!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "ratings": [1.1, null, 3.3] })),
                expected_error: Some("null value provided for non-nillable field".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_float64s_null() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { ratings: [Float64!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "ratings": null })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { ratings } }".into(),
                expected_data: json!({ "Users": [{ "ratings": null }] }),
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
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_float64s_omitted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { ratings: [Float64!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({})),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { ratings } }".into(),
                expected_data: json!({ "Users": [{ "ratings": null }] }),
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
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_ints_not_empty() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { scores: [Int!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "scores": [1, 2, 3] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { scores } }".into(),
                expected_data: json!({ "Users": [{ "scores": [1, 2, 3] }] }),
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
fn query_inline_array_with_non_nillable_ints_null_element() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { scores: [Int!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "scores": [1, null, 3] })),
                expected_error: Some("null value provided for non-nillable field".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_ints_null() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { scores: [Int!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "scores": null })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { scores } }".into(),
                expected_data: json!({ "Users": [{ "scores": null }] }),
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
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_ints_omitted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { scores: [Int!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({})),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { scores } }".into(),
                expected_data: json!({ "Users": [{ "scores": null }] }),
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
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_strings_not_empty() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { tags: [String!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "tags": ["one", "two", "three"] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { tags } }".into(),
                expected_data: json!({ "Users": [{ "tags": ["one", "two", "three"] }] }),
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
fn query_inline_array_with_non_nillable_strings_null_element() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { tags: [String!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "tags": ["one", null, "three"] })),
                expected_error: Some("null value provided for non-nillable field".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_strings_null() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { tags: [String!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "tags": null })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { tags } }".into(),
                expected_data: json!({ "Users": [{ "tags": null }] }),
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
#[mantra_macros::req_test("go.query.inline_array.non-nillable.elements-accept")]
fn query_inline_array_with_non_nillable_strings_omitted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Users { tags: [String!] }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({})),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { tags } }".into(),
                expected_data: json!({ "Users": [{ "tags": null }] }),
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
