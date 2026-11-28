//! Full-text indexes can be added to and removed from a populated collection
//! through the native and HTTP index APIs.

use integration_test::actions::shared::{IndexField, IndexGate, IndexSpec};
use integration_test::actions::{
    AddCollection, AddDoc, DeleteIndex, ListIndexes, NewIndex, NewNode, Request, Restart,
    WaitForIndexReady,
};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

const SDL: &str = "type Article { title: String  body: String  rating: Int }";
const CLIENTS: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp];
const RUST_QUERY: &str = r#"query {
    Article(order: {title: ASC}) {
        title
        score: BM25(query: "rust", fields: ["body"])
    }
}"#;

fn article(title: &str, body: &str) -> AddDoc {
    AddDoc {
        collection: "Article".into(),
        doc_map: Some(json!({ "title": title, "body": body })),
        common: Common {
            retry: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn fulltext(field: &str) -> NewIndex {
    NewIndex {
        collection: "Article".into(),
        index: IndexSpec {
            fields: vec![IndexField {
                name: field.into(),
                descending: false,
            }],
            fulltext: Some(json!({ "Language": "english", "K1": 1.4, "B": 0.7 })),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn listing() -> Value {
    json!({
        "indexes": m::unordered(json!([
            m::object(json!({
                "Description": m::object(json!({
                    "Name": "__fulltext__:body",
                    "ID": 1,
                    "Kind": 2,
                    "KindDescription": m::object(json!({
                        "FieldName": "body",
                        "Language": "english",
                        "K1": 1.4,
                        "B": 0.7,
                    })),
                    "Fields": m::unordered(json!([
                        m::object(json!({ "Name": "body", "Descending": false })),
                    ])),
                    "Unique": false,
                })),
                "Execution": m::object(json!({ "Status": 3, "Action": 0 })),
            })),
        ])),
    })
}

#[test]
#[mantra_macros::req_test("defra.index.fulltext.manage")]
fn a_populated_collection_gains_lists_and_loses_a_fulltext_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(CLIENTS),
        actions: vec![
            NewNode {
                database_dir: Some(String::new()),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            article("Compiler", "A guide to the rust compiler").into(),
            article("Baking", "Sourdough needs time and warmth").into(),
            fulltext("body").into(),
            Request {
                request: RUST_QUERY.into(),
                expected_data: json!({ "Article": [
                    { "title": "Baking", "score": 0.0 },
                    { "title": "Compiler", "score": m::positive() },
                ] }),
                ..Default::default()
            }
            .into(),
            article("Ownership", "Rust ownership after the index was built").into(),
            Request {
                request: RUST_QUERY.into(),
                expected_data: json!({ "Article": [
                    { "title": "Baking", "score": 0.0 },
                    { "title": "Compiler", "score": m::positive() },
                    { "title": "Ownership", "score": m::positive() },
                ] }),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Article".into(),
                expected_data: listing(),
                ..Default::default()
            }
            .into(),
            Restart {
                ..Default::default()
            }
            .into(),
            Request {
                request: RUST_QUERY.into(),
                expected_data: json!({ "Article": [
                    { "title": "Baking", "score": 0.0 },
                    { "title": "Compiler", "score": m::positive() },
                    { "title": "Ownership", "score": m::positive() },
                ] }),
                ..Default::default()
            }
            .into(),
            DeleteIndex {
                collection: "Article".into(),
                index_name: "__fulltext__:body".into(),
                ..Default::default()
            }
            .into(),
            Restart {
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Article".into(),
                expected_data: json!({ "indexes": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.fulltext.manage")]
fn a_fulltext_backfill_resumes_after_restart_and_indexes_writes_made_while_gated() {
    let mut index = fulltext("body");
    index.r#async = true;
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(CLIENTS),
        actions: vec![
            NewNode {
                database_dir: Some(String::new()),
                index_gate: Some(IndexGate::Single),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            article("Compiler", "A guide to the rust compiler").into(),
            index.into(),
            WaitForIndexReady {
                blocked: true,
                ..Default::default()
            }
            .into(),
            article("Ownership", "Rust ownership during a gated backfill").into(),
            Restart {
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Article".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({ "Name": "__fulltext__:body", "Kind": 2 })),
                            "Execution": m::object(json!({ "Status": 1, "Action": 3 })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
            WaitForIndexReady {
                release_gate: true,
                ..Default::default()
            }
            .into(),
            WaitForIndexReady {
                collection: Some("Article".into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: RUST_QUERY.into(),
                expected_data: json!({ "Article": [
                    { "title": "Compiler", "score": m::positive() },
                    { "title": "Ownership", "score": m::positive() },
                ] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
