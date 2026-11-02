//! Generated from the conformance scenarios of tests/integration/collection_version/vector_embedding_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::AddCollection;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.collection_version.define.embeddings")]
fn collection_version_reference_to_another_embedding_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!] @embedding(fields: ["about_v"], provider: "openai", model: "text-embedding-3-small",  url: "https://api.openai.com/v1")
                        about_v: [Float32!] @embedding(fields: ["about"], provider: "openai", model: "text-embedding-3-small",  url: "https://api.openai.com/v1")
                    }
                "#.into(),
                expected_error: Some("embedding fields cannot refer to self or another embedding field. Field: about_v".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.embeddings")]
fn collection_version_reference_to_self_for_embedding_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!] @embedding(fields: ["name_v"], provider: "ollama", model: "nomic-embed-text")
                    }
                "#.into(),
                expected_error: Some("embedding fields cannot refer to self or another embedding field. Field: name_v".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.embeddings")]
fn collection_version_with_float_for_embedding_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Users {
                        name_v: [Float!] @embedding
                    }
                "#
            .into(),
            expected_error: Some("invalid type for vector embedding".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.embeddings")]
fn collection_version_with_float32_for_embedding_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        name_v: [Float32!] @embedding(fields: ["name"], provider: "ollama", model: "nomic-embed-text",  url: "http://localhost:11434/api")
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.embeddings")]
fn collection_version_with_float64_for_embedding_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Users {
                        name_v: [Float64!] @embedding
                    }
                "#
            .into(),
            expected_error: Some("invalid type for vector embedding".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.embeddings")]
fn collection_version_with_int_for_embedding_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Users {
                        name_v: [Int!] @embedding
                    }
                "#
            .into(),
            expected_error: Some("invalid type for vector embedding".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.embeddings")]
fn collection_version_with_invalid_embedding_generation_field_type_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Users {
                        name: String
                        custom: JSON
                        name_v: [Float32!] @embedding(fields: ["name", "custom"])
                    }
                "#
            .into(),
            expected_error: Some(
                "invalid field type for vector embedding generation. Actual: JSON".into(),
            ),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.embeddings")]
fn collection_version_with_missing_model_for_embedding_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!] @embedding(fields: ["name"], provider: "ollama")
                    }
                "#
            .into(),
            expected_error: Some("embedding Model cannot be empty".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.embeddings")]
fn collection_version_with_nillable_float32_for_embedding_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Users {
                        name_v: [Float32] @embedding
                    }
                "#
            .into(),
            expected_error: Some("invalid type for vector embedding".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.embeddings")]
fn collection_version_with_non_existant_field_for_embedding_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Users {
                        name: String
                        name_v: [Float32!] @embedding(fields: ["name", "about"])
                    }
                "#
            .into(),
            expected_error: Some(
                "the given field does not exist. Embedding generation field: about".into(),
            ),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.embeddings")]
fn collection_version_with_string_for_embedding_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Users {
                        name_v: [String!] @embedding
                    }
                "#
            .into(),
            expected_error: Some("invalid type for vector embedding".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.define.embeddings")]
fn collection_version_with_unsupported_provider_for_embedding_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!] @embedding(fields: ["name"], provider: "foo")
                    }
                "#
            .into(),
            expected_error: Some("unknown embedding provider".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}
