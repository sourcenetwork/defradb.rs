//! Chunks get their vectors on every write path that creates them: an
//! encrypted parent and an upsert (`defra.chunk.embed.encrypted`,
//! `defra.chunk.embed.upsert`).

use integration_test::actions::{AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::embed_support::{
    add_embedded_collection, chunks_have_vectors, chunks_of, embedding_input, model_embedded,
};
use super::support::{body, gql};

const PARAGRAPHS: usize = 3;

fn inputs(chunks: &[String]) -> Vec<String> {
    chunks.iter().map(|chunk| embedding_input(chunk)).collect()
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.encrypted")]
fn an_encrypted_article_gets_its_chunk_vectors() {
    let chunks = chunks_of(PARAGRAPHS, "rivers");
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        AddDoc {
            collection: "Article".into(),
            doc_map: Some(json!({ "title": "t", "body": body(PARAGRAPHS, "rivers") })),
            encrypted_fields: Some(vec!["body".into()]),
            ..Default::default()
        }
        .into(),
        model_embedded(&inputs(&chunks)),
        chunks_have_vectors(&chunks, None),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.upsert")]
fn an_upsert_embeds_the_chunks_it_creates() {
    let chunks = chunks_of(PARAGRAPHS, "music");
    let text = gql(&body(PARAGRAPHS, "music"));
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        Request {
            request: format!(
                r#"mutation {{ upsert_Article(filter: {{title: {{_eq: "t"}}}}, add: {{title: "t", body: "{text}"}}, update: {{body: "{text}"}}) {{ _docID }} }}"#
            )
            .into(),
            ..Default::default()
        }
        .into(),
        model_embedded(&inputs(&chunks)),
        chunks_have_vectors(&chunks, None),
    ])
    .execute();
}
