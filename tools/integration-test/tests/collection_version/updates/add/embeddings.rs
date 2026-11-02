//! Generated from the conformance scenarios of tests/integration/collection_version/updates/add/embeddings_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, GetCollections, PatchCollection};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.field_references")]
fn col_version_update_add_vector_embedding_reference_to_another_embedding_in_patch_should_error_multiple(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                        about_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name"], "Provider": "ollama", "Model": "nomic-embed-text", "URL": "http://localhost:11434/api"}
						},
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "about_v", "Fields": ["name_v"], "Provider": "ollama", "Model": "nomic-embed-text", "URL": "http://localhost:11434/api"}
						},
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "about_v", "Fields": ["name_v"], "Provider": "ollama", "Model": "nomic-embed-text", "URL": "http://localhost:11434/api"}
						}
					]
				"#.into(),
                expected_error: Some(r#"embedding fields cannot refer to self or another embedding field. Field: name_v
invalid field type for vector embedding generation. Actual: [Float32!]
embedding fields cannot refer to self or another embedding field. Field: name_v"#.into()),
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
fn col_version_update_add_vector_embedding_reference_to_another_embedding_in_patch_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                        about_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name"], "Provider": "ollama", "Model": "nomic-embed-text", "URL": "http://localhost:11434/api"}
						},
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "about_v", "Fields": ["name_v"], "Provider": "ollama", "Model": "nomic-embed-text", "URL": "http://localhost:11434/api"}
						}
					]
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
#[mantra_macros::req_test("go.collection_version.patch.embeddings.field_references")]
fn col_version_update_add_vector_embedding_reference_to_another_embedding_should_error_multiple() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                        desc_v: [Float32!]
                        about_v: [Float32!] @embedding(fields: ["about"], provider: "openai", model: "text-embedding-3-small",  url: "https://api.openai.com/v1")
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["about_v", "about"], "Provider": "ollama", "Model": "nomic-embed-text", "URL": "http://localhost:11434/api"}
						},
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "desc_v", "Fields": ["about_v", "about"], "Provider": "ollama", "Model": "nomic-embed-text", "URL": "http://localhost:11434/api"}
						}

					]
				"#.into(),
                expected_error: Some(r#"embedding fields cannot refer to self or another embedding field. Field: about_v
invalid field type for vector embedding generation. Actual: [Float32!]
embedding fields cannot refer to self or another embedding field. Field: about_v
invalid field type for vector embedding generation. Actual: [Float32!]"#.into()),
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
fn col_version_update_add_vector_embedding_reference_to_another_embedding_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                        about_v: [Float32!] @embedding(fields: ["about"], provider: "openai", model: "text-embedding-3-small",  url: "https://api.openai.com/v1")
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["about_v", "about"], "Provider": "ollama", "Model": "nomic-embed-text", "URL": "http://localhost:11434/api"}
						}
					]
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
#[mantra_macros::req_test("go.collection_version.patch.embeddings.field_references")]
fn col_version_update_add_vector_embedding_reference_to_self_should_error_multiple() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name_v", "about"], "Provider": "ollama", "Model": "nomic-embed-text", "URL": "http://localhost:11434/api"}
						},
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name_v", "about"], "Provider": "ollama", "Model": "nomic-embed-text", "URL": "http://localhost:11434/api"}
						}
					]
				"#.into(),
                expected_error: Some(r#"embedding fields cannot refer to self or another embedding field. Field: name_v
embedding fields cannot refer to self or another embedding field. Field: name_v"#.into()),
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
fn col_version_update_add_vector_embedding_reference_to_self_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name_v", "about"], "Provider": "ollama", "Model": "nomic-embed-text", "URL": "http://localhost:11434/api"}
						}
					]
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
#[mantra_macros::req_test("go.collection_version.patch.embeddings.valid")]
fn col_version_update_add_vector_embedding_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "about"], "Provider": "ollama", "Model": "nomic-embed-text",  "URL": "http://localhost:11434/api"}
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
                                    "Fields": ["name", "about"],
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
fn col_version_update_add_vector_embedding_with_invalid_embedding_generation_field_kind_should_error(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        custom: JSON
                        name_v: [Float32!]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "custom"]}
						}
					]
				"#
                .into(),
                expected_error: Some(
                    "invalid field type for vector embedding generation. Actual: JSON".into(),
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
#[mantra_macros::req_test("go.collection_version.patch.embeddings.missing_settings")]
fn col_version_update_add_vector_embedding_with_missing_field_name_should_error_multiple() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"Fields": ["name", "about"], "Provider": "ollama", "Model": "nomic-embed-text",  "URL": "http://localhost:11434/api"}
						},
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"Fields": ["name", "about"], "Provider": "ollama", "Model": "nomic-embed-text",  "URL": "http://localhost:11434/api"}
						}
					]
				"#.into(),
                expected_error: Some(r#"embedding FieldName cannot be empty
embedding FieldName cannot be empty"#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.missing_settings")]
fn col_version_update_add_vector_embedding_with_missing_field_name_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"Fields": ["name", "about"], "Provider": "ollama", "Model": "nomic-embed-text",  "URL": "http://localhost:11434/api"}
						}
					]
				"#.into(),
                expected_error: Some("embedding FieldName cannot be empty".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.missing_settings")]
fn col_version_update_add_vector_embedding_with_missing_fields_should_error_multiple() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Provider": "ollama", "Model": "nomic-embed-text",  "URL": "http://localhost:11434/api"}
						},
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Provider": "ollama", "Model": "nomic-embed-text",  "URL": "http://localhost:11434/api"}
						}
					]
				"#.into(),
                expected_error: Some(r#"embedding Fields cannot be empty
embedding Fields cannot be empty"#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.missing_settings")]
fn col_version_update_add_vector_embedding_with_missing_fields_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Provider": "ollama", "Model": "nomic-embed-text",  "URL": "http://localhost:11434/api"}
						}
					]
				"#.into(),
                expected_error: Some("embedding Fields cannot be empty".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.missing_settings")]
fn col_version_update_add_vector_embedding_with_missing_model_should_error_multiple() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "about"], "Provider": "ollama",  "URL": "http://localhost:11434/api"}
						},
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "about"], "Provider": "ollama",  "URL": "http://localhost:11434/api"}
						}
					]
				"#.into(),
                expected_error: Some(r#"embedding Model cannot be empty
embedding Model cannot be empty"#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.missing_settings")]
fn col_version_update_add_vector_embedding_with_missing_model_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "about"], "Provider": "ollama",  "URL": "http://localhost:11434/api"}
						}
					]
				"#.into(),
                expected_error: Some("embedding Model cannot be empty".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.missing_settings")]
fn col_version_update_add_vector_embedding_with_missing_provider_should_error_multiple() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "about"], "Model": "nomic-embed-text",  "URL": "http://localhost:11434/api"}
						},
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "about"], "Model": "nomic-embed-text",  "URL": "http://localhost:11434/api"}
						}
					]
				"#.into(),
                expected_error: Some(r#"embedding Provider cannot be empty
unknown embedding provider. Provider: 
embedding Provider cannot be empty
unknown embedding provider. Provider:"#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.missing_settings")]
fn col_version_update_add_vector_embedding_with_missing_provider_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "about"], "Model": "nomic-embed-text",  "URL": "http://localhost:11434/api"}
						}
					]
				"#.into(),
                expected_error: Some("embedding Provider cannot be empty".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.valid")]
fn col_version_update_add_vector_embedding_with_missing_url_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "about"], "Provider": "ollama", "Model": "nomic-embed-text"}
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
                                    "Fields": ["name", "about"],
                                    "Model": "nomic-embed-text",
                                    "Provider": "ollama",
                                    "URL": "",
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
fn col_version_update_add_vector_embedding_with_unknown_embedding_generation_field_should_error_multiple(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "foo"]}
						},
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "bar"]}
						}
					]
				"#
                .into(),
                expected_error: Some(
                    r#"the given field does not exist. Embedding generation field: foo
the given field does not exist. Embedding generation field: bar"#
                        .into(),
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
#[mantra_macros::req_test("go.collection_version.patch.embeddings.field_references")]
fn col_version_update_add_vector_embedding_with_unknown_embedding_generation_field_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add", "path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "foo"]}
						}
					]
				"#
                .into(),
                expected_error: Some(
                    "the given field does not exist. Embedding generation field: foo".into(),
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
#[mantra_macros::req_test("go.collection_version.patch.embeddings.field_references")]
fn col_version_update_add_vector_embedding_with_unknown_field_name_should_error_multiple() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "foo"}
						},
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "bar"}
						}
					]
				"#
                .into(),
                expected_error: Some(
                    r#"the given field does not exist. Vector field: foo
the given field does not exist. Vector field: bar"#
                        .into(),
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
#[mantra_macros::req_test("go.collection_version.patch.embeddings.field_references")]
fn col_version_update_add_vector_embedding_with_unknown_field_name_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "foo"}
						}
					]
				"#
                .into(),
                expected_error: Some("the given field does not exist. Vector field: foo".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.missing_settings")]
fn col_version_update_add_vector_embedding_with_unsupported_provider_should_error_multiple() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "about"], "Provider": "deepseek", "Model": "nomic-embed-text",  "URL": "http://localhost:11434/api"}
						},
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "about"], "Provider": "deepseek", "Model": "nomic-embed-text",  "URL": "http://localhost:11434/api"}
						}
					]
				"#.into(),
                expected_error: Some(r#"unknown embedding provider. Provider: deepseek
unknown embedding provider. Provider: deepseek"#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.patch.embeddings.missing_settings")]
fn col_version_update_add_vector_embedding_with_unsupported_provider_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        about: String
                        name_v: [Float32!]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"
					[
						{
							"op": "add",
							"path": "/Users/VectorEmbeddings/-",
							"value": {"FieldName": "name_v", "Fields": ["name", "about"], "Provider": "deepseek", "Model": "nomic-embed-text",  "URL": "http://localhost:11434/api"}
						}
					]
				"#.into(),
                expected_error: Some("unknown embedding provider".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
