//! A patch can't change the chunks' embedding or embed a chunked field on
//! the parent (`defra.chunk.embed.patch`).

use integration_test::actions::{EmbeddingModel, PatchCollection};
use integration_test::test_case::TestCase;

use super::embed_support::add_embedded_collection;

fn refused(patch: &str, error: &str) -> PatchCollection {
    PatchCollection {
        patch: patch.into(),
        expected_error: Some(error.into()),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.patch")]
fn a_patch_cannot_move_or_change_a_chunk_embedding() {
    TestCase::from_actions(vec![
        EmbeddingModel::default().into(),
        add_embedded_collection(None),
        refused(
            r#"[{"op":"replace","path":"/Article__body_chunk/VectorEmbeddings/0/Model","value":"other"}]"#,
            "a generated chunk collection's embeddings cannot be mutated. Collection: Article__body_chunk",
        )
        .into(),
        refused(
            r#"[{"op":"remove","path":"/Article__body_chunk/VectorEmbeddings/0"}]"#,
            "a generated chunk collection's embeddings cannot be mutated. Collection: Article__body_chunk",
        )
        .into(),
        refused(
            r#"[{"op":"add","path":"/Article/VectorEmbeddings/-","value":{"FieldName":"title","Fields":["body"],"Model":"m","Provider":"openai"}}]"#,
            "an embedding over a chunked field belongs to its chunk collection. Field: Article.title",
        )
        .into(),
    ])
    .execute();
}
