//! `@index(vector: {maintenance: DEFERRED})`: a write records a pending entry
//! and a drain applies it to the graph later, while searches score the pending
//! entries so a committed write is found right away. Ported from Go's
//! tests/integration/index/vector_maintenance_test.go and vector_deferred_test.go
//! at sourcenetwork/defradb e152b5cab (sourcenetwork/defradb#5326).
//!
//! Go clients are left out: the Go release the go-http client runs
//! (`GO_COMPAT_TAG`, v1.2.0) predates the mode and refuses its SDL.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, ListIndexes, Parallel, Request, Restart, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

/// The clients whose servers know the mode.
const CLIENTS: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli];

fn sdl(maintenance: &str) -> String {
    format!(
        "type User {{ name: String vector: [Float32!] @index(vector: {{dimensions: 3, {maintenance}hnsw: {{metric: COSINE}}}}) }}"
    )
}

/// The listed index, whose vector description is exactly `kind`.
fn listed(kind: Value) -> Value {
    json!({
        "indexes": [m::object(json!({
            "Description": m::object(json!({
                "Name": "User_vector_ASC",
                "Kind": 1,
                "KindDescription": kind,
            })),
        }))],
    })
}

fn hnsw(maintenance: Option<&str>) -> Value {
    let mut kind = json!({
        "Algorithm": "HNSW",
        "Metric": "COSINE",
        "Dimensions": 3,
        "HNSW": {"M": 16, "EfConstruction": 128, "EfSearch": 64},
    });
    if let Some(maintenance) = maintenance {
        kind["Maintenance"] = json!(maintenance);
    }
    kind
}

fn add(name: &str, vector: Value) -> integration_test::actions::Actions {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": name, "vector": vector })),
        ..Default::default()
    }
    .into()
}

fn sim(vector: Value) -> Value {
    m::similarity_score("COSINE", vector, json!([1, 0, 0]))
}

/// The `limit` nearest to `[1, 0, 0]`, nearest first, through the index.
fn nearest(limit: usize, expected: Value) -> integration_test::actions::Actions {
    Request {
        request: format!(
            "query {{ User(order: {{_alias: {{sim: DESC}}}}, limit: {limit}) {{ name sim: SIMILARITY(vector: {{vector: [1, 0, 0]}}) }} }}"
        )
        .into(),
        expected_data: json!({ "User": expected }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("go.index.vector.maintenance.mode")]
fn vector_index_create_with_deferred_maintenance_in_sdl_is_stored() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(CLIENTS),
        actions: vec![
            AddCollection {
                sdl: sdl("maintenance: DEFERRED, ").into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: listed(hnsw(Some("DEFERRED"))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.vector.maintenance.mode")]
fn vector_index_create_without_maintenance_defaults_to_sync() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(CLIENTS),
        actions: vec![
            AddCollection {
                sdl: sdl("").into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: listed(hnsw(None)),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.vector.maintenance.mode")]
fn vector_index_create_with_explicit_sync_maintenance_is_stored_as_default() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(CLIENTS),
        actions: vec![
            AddCollection {
                sdl: sdl("maintenance: SYNC, ").into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: listed(hnsw(None)),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.vector.maintenance.mode")]
fn vector_index_create_with_unknown_maintenance_in_sdl_is_rejected() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(CLIENTS),
        actions: vec![AddCollection {
            sdl: sdl("maintenance: LAZY, ").into(),
            expected_error: Some(r#"Expected type "VectorIndexMaintenance", found LAZY"#.into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.vector.maintenance.deferred")]
fn vector_index_deferred_add_update_delete_search_is_correct() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(CLIENTS),
        actions: vec![
            AddCollection {
                sdl: sdl("maintenance: DEFERRED, ").into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "d0", "vector": [1.0, 0.0, 0.0] })),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            add("d1", json!([0.9, 0.1, 0.0])).into(),
            add("d2", json!([0.8, 0.3, 0.1])).into(),
            add("d3", json!([0.5, 0.5, 0.5])).into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "d4", "vector": [0.0, 1.0, 0.0] })),
                common: Common {
                    capture: captures([("doc4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            add("d5", json!([0.0, 0.0, 1.0])).into(),
            add("d6", json!([0.2, 0.2, 0.9])).into(),
            add("d7", json!([-1.0, 0.0, 0.0])).into(),
            // d4 moves next to the query; d0, the best match, is deleted.
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc4"),
                doc_map: json!({ "vector": [0.95, 0.05, 0.0] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"mutation { delete_User(docID: [""#),
                    Text::reference("doc0"),
                    Text::from(r#""]) { _docID } }"#),
                ]),
                expected_data: json!({ "delete_User": [{ "_docID": m::r("doc0") }] }),
                ..Default::default()
            }
            .into(),
            nearest(
                3,
                json!([
                    { "name": "d4", "sim": sim(json!([0.95, 0.05, 0])) },
                    { "name": "d1", "sim": sim(json!([0.9, 0.1, 0])) },
                    { "name": "d2", "sim": sim(json!([0.8, 0.3, 0.1])) },
                ]),
            )
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.vector.maintenance.deferred")]
fn vector_index_deferred_concurrent_writers_all_searchable() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(CLIENTS),
        actions: vec![
            AddCollection {
                sdl: sdl("maintenance: DEFERRED, ").into(),
                ..Default::default()
            }
            .into(),
            Parallel {
                actions: (0..8)
                    .map(|i| add(&format!("d{i}"), json!([1.0, f64::from(i) / 10.0, 0.0])))
                    .collect(),
                ..Default::default()
            }
            .into(),
            nearest(
                8,
                json!((0..8)
                    .map(|i| json!({
                        "name": format!("d{i}"),
                        "sim": sim(json!([1.0, f64::from(i) / 10.0, 0.0])),
                    }))
                    .collect::<Vec<_>>()),
            )
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.vector.maintenance.deferred")]
fn vector_index_deferred_writes_are_searchable_after_a_restart() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(CLIENTS),
        actions: vec![
            AddCollection {
                sdl: sdl("maintenance: DEFERRED, ").into(),
                ..Default::default()
            }
            .into(),
            add("x", json!([1.0, 0.0, 0.0])).into(),
            add("xy", json!([0.9, 0.4, 0.0])).into(),
            add("y", json!([0.0, 1.0, 0.0])).into(),
            Restart {
                ..Default::default()
            }
            .into(),
            nearest(
                2,
                json!([
                    { "name": "x", "sim": sim(json!([1.0, 0.0, 0.0])) },
                    { "name": "xy", "sim": sim(json!([0.9, 0.4, 0.0])) },
                ]),
            )
            .into(),
            add("z", json!([0.95, 0.2, 0.0])).into(),
            nearest(
                2,
                json!([
                    { "name": "x", "sim": sim(json!([1.0, 0.0, 0.0])) },
                    { "name": "z", "sim": sim(json!([0.95, 0.2, 0.0])) },
                ]),
            )
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
