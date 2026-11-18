//! Signing peers that write the same text show each of its chunks once (`defra.chunk.replicate.signed`).

use integration_test::actions::{Actions, DisconnectPeers, Request};
use integration_test::scenario::{Common, Text};
use serde_json::{json, Value};

use super::replicate_concurrent::{connect, sync, two_nodes};
use super::support::{
    add_article_on, body, eventually, on, texts, update_article_on, without_default_node, CHUNKS,
};

const PARAGRAPHS: usize = 3;

fn disconnect() -> Actions {
    DisconnectPeers {
        targets: vec!["db1".into()],
        common: on(Some("db0")),
        ..Default::default()
    }
    .into()
}

/// Names the chunk documents `db0` shows `chunk0` to `chunk<n-1>`.
fn read_chunk_ids() -> Actions {
    let common = Common {
        capture: (0..PARAGRAPHS)
            .map(|i| (format!("chunk{i}"), format!("/data/{CHUNKS}/{i}/_docID")))
            .collect(),
        ..on(Some("db0"))
    };
    Request {
        request: format!("query {{ {CHUNKS} {{ _docID }} }}").into(),
        expected_data: json!({ CHUNKS: integration_test::m::length(PARAGRAPHS as u64) }),
        common,
        ..Default::default()
    }
    .into()
}

fn chunks_are(text: &str, node: &str) -> Actions {
    let rows: Vec<Value> = texts(text)
        .into_iter()
        .map(|text| json!({ "text": text }))
        .collect();
    let mut common = on(Some(node));
    common.unordered = vec![format!("/data/{CHUNKS}")];
    eventually(
        Request {
            request: format!("query {{ Article {{ body }} {CHUNKS} {{ text }} }}").into(),
            expected_data: json!({ "Article": [{ "body": text }], CHUNKS: rows }),
            common,
            ..Default::default()
        }
        .into(),
    )
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.signed")]
fn signing_peers_that_write_the_same_text_show_each_chunk_once() {
    let shared = body(PARAGRAPHS, "shared");
    let mut actions = two_nodes(true);
    actions.extend([
        add_article_on(&body(2, "start"), "article", Some("db0")),
        connect(),
        sync("Article", vec![Text::reference("article")], "db1"),
        disconnect(),
        update_article_on("article", &shared, Some("db0")),
        update_article_on("article", &shared, Some("db1")),
        read_chunk_ids(),
        connect(),
        sync("Article", vec![Text::reference("article")], "db1"),
        sync(
            CHUNKS,
            (0..PARAGRAPHS)
                .map(|i| Text::reference(&format!("chunk{i}")))
                .collect(),
            "db1",
        ),
        chunks_are(&shared, "db1"),
    ]);
    without_default_node(actions).execute();
}
