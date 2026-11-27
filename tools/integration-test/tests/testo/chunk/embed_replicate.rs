//! A peer gets each chunk's vector with the chunk and never asks the model
//! (`defra.chunk.embed.replicate`).

use integration_test::actions::{AddCollection, NewNode, WaitForPeersEvents};
use integration_test::scenario::Text;

use super::embed_support::{
    chunks_have_vectors, chunks_of, embedded_schema, embedding_input, model_embedded,
};
use super::replicate_concurrent::{connect, sync};
use super::support::{
    add_article_on, body, eventually, on, replicated_pair_with, update_article_on,
    without_default_node, CHUNKS,
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

#[test]
#[mantra_macros::req_test("defra.chunk.embed.replicate_cleanup")]
fn a_peers_merge_deletes_the_chunks_its_text_no_longer_uses() {
    let three = chunks_of(PARAGRAPHS, "music");
    let two = chunks_of(2, "music");
    let mut actions = vec![model_embedded(&[])];
    for node in ["db0", "db1"] {
        actions.push(
            NewNode {
                network: true,
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            AddCollection {
                sdl: embedded_schema(None),
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            WaitForPeersEvents {
                start: true,
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.extend([
        connect(),
        add_article_on(&body(PARAGRAPHS, "music"), "article", Some("db0")),
        model_embedded(
            &three
                .iter()
                .map(|chunk| embedding_input(chunk))
                .collect::<Vec<_>>(),
        ),
        sync("Article", vec![Text::reference("article")], "db1"),
        eventually(chunks_have_vectors(&three, Some("db1"))),
        // Only the last paragraph's chunk is new: it loses its blank line.
        update_article_on("article", &body(2, "music"), Some("db0")),
        model_embedded(&[embedding_input(&two[1])]),
        sync("Article", vec![Text::reference("article")], "db1"),
        eventually(chunks_have_vectors(&two, Some("db1"))),
        model_embedded(&[]),
    ]);
    without_default_node(actions).execute();
}
