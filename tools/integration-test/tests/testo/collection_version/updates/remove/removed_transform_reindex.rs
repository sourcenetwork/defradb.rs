//! Removing a source transform reindexes the values an index already holds.
//!
//! Go skips its remove-transform test once its fields are indexed (Go issue
//! #4722), so these run on the Rust clients only.

use integration_test::actions::shared::{Lens, LensModule};
use integration_test::actions::{AddCollection, AddDoc, PatchCollection, Request};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

const RUST_CLIENTS: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli];

#[test]
#[mantra_macros::req_test("defra.collection_version.removed_transform_reindex")]
fn remove_source_transform_reindexes_an_indexed_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(RUST_CLIENTS),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad" })),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
                    [
                        { "op": "add", "path": "/Users/Fields/-", "value": {"Name": "email", "Kind": 11} }
                    ]
                "#
                .into(),
                lens: Some(Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "dst": "name", "value": "Fred" }),
                        path: "bin/lenses/set-default.wasm".into(),
                    }]),
                }),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
                    [
                        { "op": "remove", "path": "/Users/PreviousVersion/Transform" }
                    ]
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users(filter: {name: {_eq: "Shahzad"}}) {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Shahzad" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.collection_version.removed_transform_reindex")]
fn removing_transform_preserves_old_and_new_documents() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(RUST_CLIENTS),
        actions: vec![
            AddCollection {
                sdl: "type Users { name: String age: Int @index }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "old", "age": 20 })),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{"op":"add","path":"/Users/Fields/-","value":{"Name":"level","Kind":"Int"}}]"#
                    .into(),
                lens: Some(Lens {
                    lenses: Some(vec![LensModule {
                        arguments: json!({ "field": "age", "value": 5 }),
                        path: "bin/lenses/increment.wasm".into(),
                    }]),
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users(filter: {age: {_eq: 25}}) { name age } }".into(),
                expected_data: json!({ "Users": [{ "name": "old", "age": 25 }] }),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "new", "age": 40 })),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{"op":"remove","path":"/Users/PreviousVersion/Transform"}]"#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users(filter: {age: {_eq: 20}}) { name age } }".into(),
                expected_data: json!({ "Users": [{ "name": "old", "age": 20 }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users(filter: {age: {_eq: 40}}) { name age } }".into(),
                expected_data: json!({ "Users": [{ "name": "new", "age": 40 }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
