//! Changing a text and changing it back shows the original chunk documents again (`defra.chunk.write.revert`).

use integration_test::actions::{Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::Text;
use integration_test::test_case::TestCase;
use serde_json::json;

use super::support::{add_article, add_article_collection, body, on, CHUNKS};

fn chunk_docs(expected: serde_json::Value) -> integration_test::actions::Actions {
    let mut common = on(None);
    common.unordered = vec![format!("/data/{CHUNKS}")];
    Request {
        request: format!("query {{ {CHUNKS} {{ _docID hash text }} }}").into(),
        expected_data: json!({ CHUNKS: expected }),
        common,
        ..Default::default()
    }
    .into()
}

fn edit(text: &str) -> integration_test::actions::Actions {
    UpdateDoc {
        collection: "Article".into(),
        doc_id: Text::reference("article"),
        doc_map: json!({ "body": text }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.revert")]
fn reverting_the_text_reuses_the_original_chunks() {
    let original = |distinct: bool| {
        (0..5)
            .map(|i| {
                let id = m::same(&format!("doc{i}"));
                json!({
                    "_docID": if distinct { m::all(json!([id, m::unique("ids")])) } else { id },
                    "hash": m::same(&format!("hash{i}")),
                    "text": m::same(&format!("text{i}")),
                })
            })
            .collect::<Vec<_>>()
    };
    let other = json!({ "_docID": m::unique("ids"), "hash": m::nonempty_string(), "text": m::nonempty_string() });
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&body(5, "rivers"), "article"),
        chunk_docs(json!(original(true))),
        edit(&body(3, "mountains")),
        chunk_docs(json!([other, other, other])),
        edit(&body(5, "rivers")),
        chunk_docs(json!(original(false))),
    ])
    .execute();
}
