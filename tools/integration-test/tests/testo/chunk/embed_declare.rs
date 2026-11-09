//! An `@embedding` of a `@chunk` field lives on the chunks, and embeds that
//! field alone (`defra.chunk.embed.declare`, `defra.chunk.embed.declare_only_field`).

use integration_test::actions::{AddCollection, EmbeddingModel, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::embed_support::add_embedded_collection;
use super::support::CHUNKS;

#[test]
#[mantra_macros::req_test("defra.chunk.embed.declare")]
fn the_embedding_moves_from_the_parent_to_its_chunks() {
    TestCase::from_actions(vec![
        EmbeddingModel::default().into(),
        add_embedded_collection(None),
        Request {
            request: format!("query {{ {CHUNKS} {{ body_v }} }}").into(),
            expected_data: json!({ CHUNKS: [] }),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Article { body_v } }".into(),
            expected_error: Some(r#"Cannot query field "body_v" on type "Article""#.into()),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.declare_only_field")]
fn an_embedding_of_a_chunked_field_and_another_is_refused() {
    TestCase::from_actions(vec![AddCollection {
        sdl: r#"type Article {
            title: String
            body: String @chunk(maxChars: 64)
            body_v: [Float32!] @embedding(fields: ["title", "body"], provider: "openai", model: "m")
        }"#
        .into(),
        expected_error: Some(
            "@embedding over a @chunk field must embed only that field. Field: Article.body_v"
                .into(),
        ),
        ..Default::default()
    }
    .into()])
    .execute();
}
