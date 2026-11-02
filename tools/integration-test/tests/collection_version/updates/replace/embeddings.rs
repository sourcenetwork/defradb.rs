//! Generated from the conformance scenarios of tests/integration/collection_version/updates/replace/embeddings_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, GetCollections, PatchCollection};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.valid")]
fn col_version_update_replace_vector_embedding_params_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!] @embedding(fields: ["name"], provider: "openai", model: "text-embedding-3-small",  url: "https://api.openai.com/v1")
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "replace",
							"path": "/Users/VectorEmbeddings/0/Fields",
							"value": ["about"]
						},
						{
							"op": "replace",
							"path": "/Users/VectorEmbeddings/0/Provider",
							"value": "ollama"
						},
						{
							"op": "replace",
							"path": "/Users/VectorEmbeddings/0/Model",
							"value": "nomic-embed-text"
						},
						{
							"op": "replace",
							"path": "/Users/VectorEmbeddings/0/URL",
							"value": "http://localhost:11434/api"
						}
					]
				"#.into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                expected_data: json!({
                    "collections": [
                        m::object(json!({
                            "IsActive": true,
                            "IsMaterialized": true,
                            "IsBranchable": false,
                            "Name": "Users",
                            "PreviousVersion": null,
                            "VectorEmbeddings": [
                                {
                                    "FieldName": "name_v",
                                    "Fields": ["about"],
                                    "Model": "nomic-embed-text",
                                    "Provider": "ollama",
                                    "URL": "http://localhost:11434/api",
                                    "Template": "",
                                },
                            ],
                        })),
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

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.field_references")]
fn col_version_update_replace_vector_embedding_with_invalid_embedding_generation_field_kind_should_error(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        custom: JSON
                        name_v: [Float32!] @embedding(fields: ["name"], provider: "openai", model: "text-embedding-3-small",  url: "https://api.openai.com/v1")
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "replace",
							"path": "/Users/VectorEmbeddings/0/Fields",
							"value": ["name", "custom"]
						}
					]
				"#.into(),
                expected_error: Some("invalid field type for vector embedding generation. Actual: JSON".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.field_references")]
fn col_version_update_replace_vector_embedding_with_unknown_embedding_generation_field_should_error(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!] @embedding(fields: ["name"], provider: "openai", model: "text-embedding-3-small",  url: "https://api.openai.com/v1")
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "replace",
							"path": "/Users/VectorEmbeddings/0/Fields",
							"value": ["name", "foo"]
						}
					]
				"#.into(),
                expected_error: Some("the given field does not exist. Embedding generation field: foo".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.field_references")]
fn col_version_update_replace_vector_embedding_with_unknown_field_name_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!] @embedding(fields: ["name"], provider: "openai", model: "text-embedding-3-small",  url: "https://api.openai.com/v1")
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "replace",
							"path": "/Users/VectorEmbeddings/0/FieldName",
							"value": "foo"
						}
					]
				"#.into(),
                expected_error: Some("the given field does not exist. Vector field: foo".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
