//! An edit deletes the chunks its new text no longer uses and keeps the rest (`defra.chunk.write.cleanup`).

use integration_test::actions::Request;
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

use super::support::{
    add_article, add_article_collection, body, on, texts, update_article, CHUNKS,
};

fn chunk_docs(expected: Vec<Value>) -> integration_test::actions::Actions {
    let mut common = on(None);
    common.unordered = vec![format!("/data/{CHUNKS}")];
    Request {
        request: format!("query {{ {CHUNKS} {{ _docID text generation }} }}").into(),
        expected_data: json!({ CHUNKS: expected }),
        common,
        ..Default::default()
    }
    .into()
}

/// A chunk of `text` at `generation`. `id` names its `_docID`; any other is new.
fn chunk(text: &Value, generation: Value, id: Option<&str>) -> Value {
    json!({
        "_docID": id.map_or_else(|| m::unique("ids"), m::same),
        "text": text,
        "generation": generation,
    })
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.cleanup")]
fn an_edit_deletes_the_chunks_it_no_longer_uses() {
    let rivers = body(3, "rivers");
    let first = texts(&rivers).remove(0);
    let kept = first.as_str().unwrap().to_owned();
    let edited = format!("{kept}{}", body(2, "mountains"));
    let mut originals = texts(&rivers);
    let dropped = originals.split_off(1);
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&rivers, "article"),
        chunk_docs(
            std::iter::once(chunk(&first, json!(null), Some("kept")))
                .chain(dropped.iter().map(|text| chunk(text, json!(null), None)))
                .collect(),
        ),
        update_article("article", &edited),
        chunk_docs(
            std::iter::once(chunk(&first, json!(null), Some("kept")))
                .chain(
                    texts(&body(2, "mountains"))
                        .iter()
                        .map(|text| chunk(text, json!(null), None)),
                )
                .collect(),
        ),
        // The kept chunk was never deleted; the others were, so they come back new.
        update_article("article", &rivers),
        chunk_docs(
            std::iter::once(chunk(&first, json!(null), Some("kept")))
                .chain(dropped.iter().map(|text| chunk(text, json!(1), None)))
                .collect(),
        ),
    ])
    .execute();
}
