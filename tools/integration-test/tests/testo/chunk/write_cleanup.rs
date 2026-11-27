//! An edit deletes the chunks its new text no longer uses and keeps the rest,
//! and a null text deletes them all (`defra.chunk.write.cleanup`).

use integration_test::actions::{Actions, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::Text;
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

use super::support::{
    add_article, add_article_collection, body, no_chunks, on, texts, update_article, CHUNKS,
};

fn chunk_docs(expected: Vec<Value>) -> Actions {
    let mut common = on(None);
    common.unordered = vec![format!("/data/{CHUNKS}")];
    Request {
        request: format!("query {{ {CHUNKS} {{ _docID text sourceCID }} }}").into(),
        expected_data: json!({ CHUNKS: expected }),
        common,
        ..Default::default()
    }
    .into()
}

fn chunk(text: &Value, source: Value, id: Value) -> Value {
    json!({ "_docID": id, "text": text, "sourceCID": source })
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.cleanup")]
fn an_edit_deletes_the_chunks_it_no_longer_uses() {
    let rivers = body(3, "rivers");
    let first = texts(&rivers).remove(0);
    let kept = first.as_str().unwrap().to_owned();
    let mut originals = texts(&rivers);
    let dropped = originals.split_off(1);
    let created = || m::all(json!([m::cid(), m::same("created")]));
    let edited = || m::all(json!([m::same("edited"), m::not(m::same("created"))]));
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&rivers, "article"),
        chunk_docs(
            std::iter::once(chunk(&first, created(), m::same("kept")))
                .chain(
                    dropped
                        .iter()
                        .map(|text| chunk(text, created(), m::unique("ids"))),
                )
                .collect(),
        ),
        update_article("article", &format!("{kept}{}", body(2, "mountains"))),
        chunk_docs(
            std::iter::once(chunk(&first, created(), m::same("kept")))
                .chain(
                    texts(&body(2, "mountains"))
                        .iter()
                        .map(|text| chunk(text, edited(), m::unique("ids"))),
                )
                .collect(),
        ),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.cleanup")]
fn a_null_text_deletes_every_chunk() {
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&body(3, "rivers"), "article"),
        UpdateDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            doc_map: json!({ "body": null }),
            ..Default::default()
        }
        .into(),
        no_chunks(None),
    ])
    .execute();
}
