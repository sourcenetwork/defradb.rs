use integration_test::actions::{EmbeddingModel, Request};
use integration_test::scenario::{Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::embed_failure::MODEL_FAILED;
use super::embed_support::{
    add_embedded_collection, chunks_have_vectors, chunks_of, embedding_input, model_embedded,
};
use super::support::{add_article, gql, update_article};

#[test]
#[mantra_macros::req_test("defra.chunk.embed.batches")]
fn a_large_edit_embeds_all_its_chunks_in_bounded_batches() {
    let initial = chunks_of(1, "music");
    let edited = chunks_of(1025, "rivers");
    let expected: Vec<_> = edited.iter().map(|chunk| embedding_input(chunk)).collect();
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        add_article(&initial.concat(), "article"),
        model_embedded(&[embedding_input(&initial[0])]),
        update_article("article", &edited.concat()),
        EmbeddingModel {
            expected_max_batch: Some(256),
            expected_data: json!({ "embedded": expected }),
            common: Common {
                unordered: vec!["/embedded".into()],
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        chunks_have_vectors(&edited, None),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.embed.batches")]
fn a_failed_batch_leaves_the_article_and_chunks_unchanged() {
    let initial = chunks_of(1, "music");
    let edited = chunks_of(257, "rivers");
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        add_article(&initial.concat(), "article"),
        EmbeddingModel {
            reject_text: Some("Paragraph 256 ".into()),
            ..Default::default()
        }
        .into(),
        Request {
            request: Text::concat([
                Text::from(r#"mutation { update_Article(docID: ""#),
                Text::reference("article"),
                Text::from(format!(
                    r#"", input: {{body: "{}"}}) {{ _docID }} }}"#,
                    gql(&edited.concat())
                )),
            ]),
            expected_error: Some(MODEL_FAILED.into()),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Article { body } }".into(),
            expected_data: json!({ "Article": [{ "body": initial.concat() }] }),
            ..Default::default()
        }
        .into(),
        chunks_have_vectors(&initial, None),
    ])
    .execute();
}
