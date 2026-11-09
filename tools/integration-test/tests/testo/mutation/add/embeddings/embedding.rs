//! Generated from the conformance scenarios of tests/integration/mutation/add/embeddings/embedding_test.go at defradb 307364d9d828.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, WaitForPeersEvents, WaitForSync};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.embeddings.skip")]
fn mutation_add_user_defined_vector_embedding_does_not_trigger_generation_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
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
                doc_map: Some(json!({ "name": "John", "about": "He loves tacos.", "name_v": [1, 2, 3] })),
                common: Common { capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User {
                            _docID
                            name_v
                        }
                    }
                "#.into(),
                expected_data: json!({ "User": [{ "name_v": [1.0, 2.0, 3.0], "_docID": m::r("doc0") }] }),
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
fn mutation_add_with_multiple_embedding_fields_should_succeed() {
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
                doc_map: Some(json!({ "name": "John", "about": "He loves tacos." })),
                common: Common { retry: true, capture: captures([("doc0", "/docID")]), ..Default::default() },
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
                doc_map: Some(json!({ "name": "John" })),
                common: Common { retry: true, capture: captures([("doc1", "/docID")]), ..Default::default() },
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
        client_types: Some(&[ClientType::InProcess]),
        ..Default::default()
    }
    .execute();
}
