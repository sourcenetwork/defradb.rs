use integration_test::actions::{AddCollection, AddDoc, PatchCollection, Request};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.index.patch_indexed_field")]
fn removing_an_indexed_field_names_the_index_and_keeps_the_collection_usable() {
    TestCase {
        id: integration_test::scenario_id!(),
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
            PatchCollection {
                patch: r#"[{"op":"remove","path":"/Users/Fields/1"}]"#.into(),
                expected_error: Some(
                    "index 'Users_name_ASC' references field 'name' which does not exist in schema"
                        .into(),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Users {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({"Users": []}),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({"name": "ann"})),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.patch_indexed_field")]
fn removing_a_fulltext_field_names_the_field_and_keeps_the_collection_usable() {
    TestCase {
        id: integration_test::scenario_id!(),
        // Go's pinned schema parser does not support @fulltext.
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Notes {
                        body: String @fulltext
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            PatchCollection {
                patch: r#"[{"op":"remove","path":"/Notes/Fields/1"}]"#.into(),
                expected_error: Some(
                    "full-text index on field 'body' references a field which does not exist in schema"
                        .into(),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Notes".into(),
                doc_map: Some(json!({"body": "still writable"})),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.patch_indexed_field")]
fn removing_an_embedding_source_keeps_the_collection_usable() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        name_v: [Float32!] @embedding(fields: ["name"], provider: "ollama", model: "nomic-embed-text", url: "http://localhost:11434/api")
                    }
                "#.into(),
                ..Default::default()
            }.into(),
            PatchCollection {
                patch: r#"[{"op":"remove","path":"/Users/Fields/1"}]"#.into(),
                expected_error: Some("the given field does not exist. Embedding generation field: name".into()),
                ..Default::default()
            }.into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({"name": "ann", "name_v": [1, 2, 3]})),
                ..Default::default()
            }.into(),
        ],
        ..Default::default()
    }.execute();
}
