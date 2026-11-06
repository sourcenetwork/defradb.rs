//! Concurrent updates, and a delete, of one parent leave exactly the chunks of the text that won (`defra.chunk.write.concurrent`).

use integration_test::actions::{Actions, DeleteDoc, Parallel, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

use super::support::{add_article, add_article_collection, body, CHUNKS};

fn paragraphs(i: usize) -> usize {
    3 + i % 4
}

fn candidate_body(i: usize) -> String {
    body(paragraphs(i), &format!("topic {i}"))
}

/// A chunk keeps the blank line that follows its paragraph.
fn candidate_chunks(i: usize) -> Value {
    let texts: Vec<Value> = candidate_body(i)
        .split_inclusive("\n\n")
        .map(|text| json!({ "text": text }))
        .collect();
    m::unordered(json!(texts))
}

/// The state after update `i` won: its text, its chunk list and its chunks, no others.
fn won_by(i: usize) -> Value {
    json!({
        "Article": [{ "body": candidate_body(i), "_body_chunks": m::length(paragraphs(i) as u64 + 1) }],
        CHUNKS: candidate_chunks(i),
    })
}

fn update(i: usize) -> Actions {
    UpdateDoc {
        collection: "Article".into(),
        doc_id: Text::reference("article"),
        doc_map: json!({ "body": candidate_body(i) }),
        common: Common {
            envelope: true,
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn read_state(expected: Value) -> Actions {
    Request {
        request: format!("query {{ Article {{ body _body_chunks }} {CHUNKS} {{ text }} }}").into(),
        expected_data: expected,
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.concurrent")]
fn concurrent_updates_leave_the_chunks_of_the_winning_text() {
    let winners: Vec<Value> = (0..8).map(won_by).collect();
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&body(4, "start"), "article"),
        Parallel {
            actions: (0..8).map(update).collect(),
            ..Default::default()
        }
        .into(),
        read_state(m::any_of(json!(winners))),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.write.concurrent")]
fn concurrent_updates_and_a_delete_leave_no_stray_chunk() {
    let mut outcomes: Vec<Value> = (0..6).map(won_by).collect();
    outcomes.push(json!({ "Article": [], CHUNKS: [] }));
    let mut writes: Vec<Actions> = (0..6).map(update).collect();
    writes.push(
        DeleteDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            common: Common {
                envelope: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    );
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&body(4, "start"), "article"),
        Parallel {
            actions: writes,
            ..Default::default()
        }
        .into(),
        read_state(m::any_of(json!(outcomes))),
    ])
    .execute();
}
