//! A peer deletes and remakes the chunks its parent's edits drop and bring back, as the writer does (`defra.chunk.replicate.cleanup`).

use integration_test::actions::{Actions, Request};
use integration_test::m;
use integration_test::scenario::Text;
use serde_json::{json, Value};

use super::replicate_concurrent::{connect, sync, two_nodes};
use super::support::{
    add_article_on, body, eventually, on, replicated_pair, texts, update_article_on,
    without_default_node, CHUNKS,
};

/// `node` shows `text` and exactly its chunks at `generation`, each `_docID`
/// named `<prefix><i>` so another node's read must match it.
fn shows(node: &str, text: &str, generation: Value, prefix: &str) -> Actions {
    let rows: Vec<Value> = texts(text)
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            json!({
                "_docID": m::same(&format!("{prefix}{i}")),
                "text": chunk,
                "generation": generation,
            })
        })
        .collect();
    let mut common = on(Some(node));
    common.unordered = vec![format!("/data/{CHUNKS}")];
    eventually(
        Request {
            request: format!(
                "query {{ Article {{ body }} {CHUNKS} {{ _docID text generation }} }}"
            )
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
        shows("db0", &rivers, json!(null), "rivers"),
        shows("db1", &rivers, json!(null), "rivers"),
        update_article_on("article", &mountains, Some("db0")),
        shows("db0", &mountains, json!(null), "mountains"),
        shows("db1", &mountains, json!(null), "mountains"),
        update_article_on("article", &rivers, Some("db0")),
        shows("db0", &rivers, json!(1), "again"),
        shows("db1", &rivers, json!(1), "again"),
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
        // db0 builds south's chunks under the IDs db1 wrote them with.
        shows("db1", &south, json!(null), "south"),
        shows("db0", &south, json!(null), "south"),
        // Both deleted start's chunks, so both make them again at generation 1.
        update_article_on("article", &start, Some("db0")),
        shows("db0", &start, json!(1), "again"),
        sync("Article", vec![Text::reference("article")], "db1"),
        shows("db1", &start, json!(1), "again"),
    ]);
    without_default_node(actions).execute();
}
