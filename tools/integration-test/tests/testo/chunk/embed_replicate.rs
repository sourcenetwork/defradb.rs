//! A peer gets each chunk's vector with the chunk and never asks the model
//! (`defra.chunk.embed.replicate`).

use super::embed_support::{
    chunks_have_vectors, chunks_of, embedded_schema, embedding_input, model_embedded,
};
use super::support::{
    add_article_on, body, eventually, replicated_pair_with, without_default_node, CHUNKS,
};

const PARAGRAPHS: usize = 3;

#[test]
#[mantra_macros::req_test("defra.chunk.embed.replicate")]
fn a_peer_gets_the_vectors_without_calling_the_model() {
    let chunks = chunks_of(PARAGRAPHS, "music");
    let inputs: Vec<String> = chunks.iter().map(|chunk| embedding_input(chunk)).collect();
    let mut actions = vec![model_embedded(&[])];
    actions.extend(replicated_pair_with(
        embedded_schema(None),
        &["Article", CHUNKS],
    ));
    actions.extend([
        add_article_on(&body(PARAGRAPHS, "music"), "article", Some("db0")),
        eventually(chunks_have_vectors(&chunks, Some("db1"))),
        model_embedded(&inputs),
    ]);
    without_default_node(actions).execute();
}
