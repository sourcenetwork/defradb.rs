//! Changing a text and changing it back makes its deleted chunks again under a new generation (`defra.chunk.write.revert`).

use integration_test::actions::{Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::Text;
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

use super::support::{add_article, add_article_collection, body, on, CHUNKS};

fn chunk_docs(expected: Value) -> integration_test::actions::Actions {
    let mut common = on(None);
    common.unordered = vec![format!("/data/{CHUNKS}")];
    Request {
        request: format!("query {{ {CHUNKS} {{ _docID hash text generation }} }}").into(),
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

/// The five chunks of the first body at `generation`, each with a `_docID` no other chunk had.
fn original(generation: Value) -> Value {
    json!((0..5)
        .map(|i| json!({
            "_docID": m::unique("ids"),
            "hash": m::same(&format!("hash{i}")),
            "text": m::same(&format!("text{i}")),
            "generation": generation,
        }))
        .collect::<Vec<_>>())
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.revert")]
fn reverting_the_text_makes_its_chunks_again_under_a_new_generation() {
    let other = json!({
        "_docID": m::unique("ids"),
        "hash": m::nonempty_string(),
        "text": m::nonempty_string(),
        "generation": null,
    });
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&body(5, "rivers"), "article"),
        chunk_docs(original(json!(null))),
        edit(&body(3, "mountains")),
        chunk_docs(json!([other, other, other])),
        edit(&body(5, "rivers")),
        chunk_docs(original(json!(1))),
        edit(&body(3, "mountains")),
        edit(&body(5, "rivers")),
        chunk_docs(original(json!(2))),
    ])
    .execute();
}
