//! Generated from the conformance scenarios of tests/integration/mutation/update/embeddings/embedding_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, Request, UpdateDoc, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.embeddings.skip")]
fn mutation_update_fields_for_embedding_not_updated_does_not_trigger_generation_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        about: String
                        age: Int
                        name_v: [Float32!] @embedding(fields: ["name", "about"], provider: "ollama", model: "nomic-embed-text",  url: "http://localhost:11434/api")
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "about": "He loves fajitas.", "name_v": [1, 2, 3] })),
                common: Common { capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0"),
                doc_map: json!({ "age": 30 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User {
                            name_v
                        }
                    }
                "#.into(),
                expected_data: json!({ "User": [{ "name_v": [1.0, 2.0, 3.0] }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.embeddings.skip")]
fn mutation_update_user_defined_vector_embedding_does_not_trigger_generation_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            WaitForPeersEvents {
                start: true,
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        about: String
                        name_v: [Float32!] @embedding(fields: ["name", "about"], provider: "ollama", model: "nomic-embed-text",  url: "http://localhost:11434/api")
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "about": "He loves fajitas." })),
                common: Common { retry: true, capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0"),
                doc_map: json!({ "name_v": [1, 2, 3] }),
                expected_data: json!({}),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User {
                            name_v
                        }
                    }
                "#.into(),
                expected_data: json!({ "User": [{ "name_v": [1.0, 2.0, 3.0] }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.embeddings.generate")]
fn mutation_update_with_multiple_embedding_fields_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            WaitForPeersEvents {
                start: true,
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        about: String
                        name_v: [Float32!] @embedding(fields: ["name", "about"], provider: "ollama", model: "nomic-embed-text",  url: "http://localhost:11434/api")
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "about": "He loves fajitas." })),
                common: Common { retry: true, capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0"),
                doc_map: json!({ "about": "He loves tacos." }),
                expected_data: json!({}),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Johnny" })),
                common: Common { retry: true, capture: captures([("doc1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc1")),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc1"),
                doc_map: json!({ "name": "John" }),
                expected_data: json!({}),
                common: Common { retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc1")),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User {
                            name_v
                        }
                    }
                "#.into(),
                result_types: Some(json!({ "/User/0/name_v": "float32Array", "/User/1/name_v": "float32Array" })),
                expected_data: json!({ "User": [{ "name_v": m::length(768) }, { "name_v": m::length(768) }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
