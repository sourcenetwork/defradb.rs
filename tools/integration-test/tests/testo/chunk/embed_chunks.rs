//! Each chunk is embedded on its own text, and an edit embeds only the chunk
//! it changes (`defra.chunk.embed.chunks`, `defra.chunk.embed.edit`).

use integration_test::actions::UpdateDoc;
use integration_test::scenario::Text;
use integration_test::test_case::TestCase;
use serde_json::json;

use super::embed_support::{
    add_embedded_collection, chunks_have_vectors, embedding_input, model_embedded, paragraph,
};
use super::support::add_article;

const TOPICS: [&str; 4] = ["rivers", "engines", "music", "rivers"];

/// One paragraph per topic, with paragraph `edited` about `music` instead.
fn chunks(edited: Option<usize>) -> Vec<String> {
    TOPICS
        .iter()
        .enumerate()
        .map(|(i, topic)| {
            let topic = if edited == Some(i) { "music" } else { topic };
            paragraph(i, topic, i + 1 == TOPICS.len())
        })
        .collect()
}

fn body(edited: Option<usize>) -> String {
    chunks(edited).concat()
}

fn inputs(chunks: &[String]) -> Vec<String> {
    chunks.iter().map(|chunk| embedding_input(chunk)).collect()
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.chunks")]
fn each_chunk_is_embedded_on_its_own_text() {
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        add_article(&body(None), "article"),
        model_embedded(&inputs(&chunks(None))),
        chunks_have_vectors(&chunks(None), None),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.edit")]
fn editing_one_paragraph_embeds_only_its_new_chunk() {
    let edited = chunks(Some(1));
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        add_article(&body(None), "article"),
        model_embedded(&inputs(&chunks(None))),
        UpdateDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            doc_map: json!({ "body": body(Some(1)) }),
            ..Default::default()
        }
        .into(),
        model_embedded(&[embedding_input(&edited[1])]),
        chunks_have_vectors(&edited, None),
    ])
    .execute();
}
