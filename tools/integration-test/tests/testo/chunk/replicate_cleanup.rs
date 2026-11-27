//! A peer deletes and remakes the chunks its parent's edits drop and bring
//! back, as the writer does (`defra.chunk.replicate.cleanup`).

use integration_test::actions::{Actions, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::Text;
use serde_json::{json, Value};

use super::replicate_concurrent::{connect, sync, two_nodes};
use super::support::{
    add_article_on, body, eventually, no_chunks, on, read_chunks, replicated_pair, texts,
    update_article_on, without_default_node, CHUNKS,
};

/// `node` shows `text` and exactly its chunks, each `_docID` and their
/// `sourceCID` named after `prefix`, so another node's read must match them.
pub(super) fn shows(node: &str, text: &str, prefix: &str) -> Actions {
    let rows: Vec<Value> = texts(text)
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            json!({
                "_docID": m::same(&format!("{prefix}{i}")),
                "text": chunk,
                "sourceCID": m::same(&format!("{prefix}source")),
            })
        })
        .collect();
    let mut common = on(Some(node));
    common.unordered = vec![format!("/data/{CHUNKS}")];
    eventually(
        Request {
            request: format!("query {{ Article {{ body }} {CHUNKS} {{ _docID text sourceCID }} }}")
                .into(),
            expected_data: json!({ "Article": [{ "body": text }], CHUNKS: rows }),
            common,
            ..Default::default()
        }
        .into(),
    )
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.cleanup")]
fn a_replicated_peer_follows_an_edit_and_its_revert() {
    let rivers = body(3, "rivers");
    let mountains = body(2, "mountains");
    let mut actions = replicated_pair(&["Article"]);
    actions.extend([
        add_article_on(&rivers, "article", Some("db0")),
        shows("db0", &rivers, "rivers"),
        shows("db1", &rivers, "rivers"),
        update_article_on("article", &mountains, Some("db0")),
        shows("db0", &mountains, "mountains"),
        shows("db1", &mountains, "mountains"),
        update_article_on("article", &rivers, Some("db0")),
        shows("db0", &rivers, "again"),
        shows("db1", &rivers, "again"),
    ]);
    without_default_node(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.cleanup")]
fn peers_that_edit_apart_and_revert_hold_the_same_chunks() {
    let start = body(2, "start");
    let north = body(3, "north");
    let south = body(3, "south");
    let mut actions = two_nodes(false);
    actions.extend([
        add_article_on(&start, "article", Some("db0")),
        add_article_on(&start, "copy", Some("db1")),
        update_article_on("article", &north, Some("db0")),
        update_article_on("article", &south, Some("db1")),
        connect(),
        sync("Article", vec![Text::reference("article")], "db0"),
        sync("Article", vec![Text::reference("article")], "db1"),
        // Two heads: both move every chunk to the winning commit.
        shows("db1", &south, "south"),
        shows("db0", &south, "south"),
        update_article_on("article", &start, Some("db0")),
        shows("db0", &start, "again"),
        sync("Article", vec![Text::reference("article")], "db1"),
        shows("db1", &start, "again"),
    ]);
    without_default_node(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.cleanup")]
fn a_null_text_deletes_a_peers_chunks() {
    let rivers = body(3, "rivers");
    let mut actions = two_nodes(false);
    actions.extend([
        add_article_on(&rivers, "article", Some("db0")),
        add_article_on(&rivers, "copy", Some("db1")),
        read_chunks("held", 3, Some("db1")),
        UpdateDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            doc_map: json!({ "body": null }),
            common: on(Some("db0")),
            ..Default::default()
        }
        .into(),
        connect(),
        sync("Article", vec![Text::reference("article")], "db1"),
        eventually(
            Request {
                request: "query { Article { body } }".into(),
                expected_data: json!({ "Article": [{ "body": null }] }),
                common: on(Some("db1")),
                ..Default::default()
            }
            .into(),
        ),
        eventually(no_chunks(Some("db1"))),
    ]);
    without_default_node(actions).execute();
}
