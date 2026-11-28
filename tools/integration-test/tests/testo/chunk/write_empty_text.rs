//! An empty body has no chunks, and a body of only whitespace is one chunk holding it (`defra.chunk.write.empty_text`).

use integration_test::actions::{Actions, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{
    add_article, add_article_collection, body, chunk_texts_are, no_chunks, update_article, CHUNKS,
};

const BLANK: &str = "   \n\n  ";

fn one_chunk_holding(text: &str) -> Actions {
    Request {
        request: format!("query {{ {CHUNKS} {{ text }} }}").into(),
        expected_data: json!({ CHUNKS: [{ "text": text }] }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.empty_text")]
fn an_empty_body_has_no_chunks_and_whitespace_is_one_chunk() {
    let hills = body(2, "hills");
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article("", "article"),
        no_chunks(None),
        update_article("article", &hills),
        chunk_texts_are(&[&hills], None),
        update_article("article", ""),
        no_chunks(None),
        update_article("article", BLANK),
        one_chunk_holding(BLANK),
        update_article("article", &hills),
        chunk_texts_are(&[&hills], None),
    ])
    .execute();
}
