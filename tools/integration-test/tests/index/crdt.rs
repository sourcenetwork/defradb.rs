//! Generated from the conformance scenarios of tests/integration/index/crdt_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{IndexField, IndexSpec};
use integration_test::actions::{AddCollection, AddDoc, NewIndex, Request};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.definition.accumulated-crdt")]
fn index_new_with_collection_level_index_on_pn_counter_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "points"}]) {
                        name: String
                        points: Int @crdt(type: pncounter)
                    }
                "#.into(),
                expected_error: Some("indexing accumulated CRDT fields is not yet supported. Field: points, CRDTType: pncounter".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.accumulated-crdt")]
fn index_new_with_composite_index_including_pn_counter_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        points: Int @crdt(type: pncounter)
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "composite_index".into(),
                    unique: false,
                    fields: vec![
                        IndexField {
                            name: "name".into(),
                            descending: false,
                        },
                        IndexField {
                            name: "points".into(),
                            descending: false,
                        },
                    ],
                    ..Default::default()
                },
                expected_error: Some("indexing accumulated CRDT fields is not yet supported. Field: points, CRDTType: pncounter".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.accumulated-crdt")]
fn index_new_with_lww_field_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "age_index".into(),
                    unique: false,
                    fields: vec![IndexField {
                        name: "age".into(),
                        descending: false,
                    }],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30 })),
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
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "age": 30, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.accumulated-crdt")]
fn index_new_with_p_counter_field_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        points: Int @crdt(type: pcounter)
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "points_index".into(),
                    unique: false,
                    fields: vec![
                        IndexField {
                            name: "points".into(),
                            descending: false,
                        },
                    ],
                    ..Default::default()
                },
                expected_error: Some("indexing accumulated CRDT fields is not yet supported. Field: points, CRDTType: pcounter".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.accumulated-crdt")]
fn index_new_with_p_counter_field_via_directive_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        points: Int @crdt(type: pcounter) @index
                    }
                "#.into(),
                expected_error: Some("indexing accumulated CRDT fields is not yet supported. Field: points, CRDTType: pcounter".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.accumulated-crdt")]
fn index_new_with_pn_counter_field_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        points: Int @crdt(type: pncounter)
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "points_index".into(),
                    unique: false,
                    fields: vec![
                        IndexField {
                            name: "points".into(),
                            descending: false,
                        },
                    ],
                    ..Default::default()
                },
                expected_error: Some("indexing accumulated CRDT fields is not yet supported. Field: points, CRDTType: pncounter".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.accumulated-crdt")]
fn index_new_with_pn_counter_field_via_directive_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        points: Int @crdt(type: pncounter) @index
                    }
                "#.into(),
                expected_error: Some("indexing accumulated CRDT fields is not yet supported. Field: points, CRDTType: pncounter".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.accumulated-crdt")]
fn index_new_with_pn_counter_float_field_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        score: Float @crdt(type: pncounter)
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "score_index".into(),
                    unique: false,
                    fields: vec![
                        IndexField {
                            name: "score".into(),
                            descending: false,
                        },
                    ],
                    ..Default::default()
                },
                expected_error: Some("indexing accumulated CRDT fields is not yet supported. Field: score, CRDTType: pncounter".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.accumulated-crdt")]
fn index_new_with_unique_index_on_pn_counter_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        points: Int @crdt(type: pncounter)
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "unique_points_index".into(),
                    unique: true,
                    fields: vec![
                        IndexField {
                            name: "points".into(),
                            descending: false,
                        },
                    ],
                    ..Default::default()
                },
                expected_error: Some("indexing accumulated CRDT fields is not yet supported. Field: points, CRDTType: pncounter".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
