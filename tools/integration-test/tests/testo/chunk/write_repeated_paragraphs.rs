//! A paragraph a text repeats has one chunk document (`defra.chunk.write.repeated_paragraphs`).

use integration_test::actions::{Actions, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{add_article, add_article_collection, on, update_article, CHUNKS};

const SAME: &str = "This paragraph is written out word for word again.";
const OTHER: &str = "This last paragraph is written only once at the end.";

fn text(repeats: usize) -> String {
    let mut paragraphs = vec![SAME; repeats];
    paragraphs.push(OTHER);
    paragraphs.join("\n\n")
}

fn chunk_docs() -> Actions {
    let mut common = on(None);
    common.unordered = vec![format!("/data/{CHUNKS}")];
    Request {
        request: format!("query {{ {CHUNKS} {{ _docID text }} }}").into(),
        expected_data: json!({ CHUNKS: [
            { "_docID": m::same("same"), "text": format!("{SAME}\n\n") },
            { "_docID": m::same("other"), "text": OTHER },
        ] }),
        common,
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.repeated_paragraphs")]
fn a_repeated_paragraph_has_one_chunk() {
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&text(2), "article"),
        chunk_docs(),
        update_article("article", &text(4)),
        chunk_docs(),
    ])
    .execute();
}
