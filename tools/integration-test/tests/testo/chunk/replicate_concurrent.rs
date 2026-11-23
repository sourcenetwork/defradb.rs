//! Two peers that edit one parent apart end with the chunks of the merged text (`defra.chunk.replicate.concurrent`).

use integration_test::actions::{
    Actions, AddCollection, ConnectPeers, NewNode, Request, SyncDocs, WaitForPeersEvents,
};
use integration_test::scenario::Text;
use serde_json::{json, Value};

use super::support::{
    add_article_on, body, eventually, on, read_list, texts, update_article_on,
    without_default_node, CHUNKS, SCHEMA,
};

// The same length, so the text tie goes to "south" while the list tie goes to
// the list of "north". Readers cut the text, so the chunks are still south's.
const NORTH: &str = "north";
const SOUTH: &str = "south";
const PARAGRAPHS: usize = 3;

pub(super) fn two_nodes(signing: bool) -> Vec<Actions> {
    let mut actions = Vec::new();
    for node in ["db0", "db1"] {
        actions.push(
            NewNode {
                network: true,
                enable_signing: signing,
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
    }
    for node in ["db0", "db1"] {
        actions.push(
            AddCollection {
                sdl: SCHEMA.into(),
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            WaitForPeersEvents {
                start: true,
                common: on(Some(node)),
                ..Default::default()
            }
            .into(),
        );
    }
    actions
}

pub(super) fn connect() -> Actions {
    ConnectPeers {
        target: "db1".into(),
        common: on(Some("db0")),
        ..Default::default()
    }
    .into()
}

/// Pulls the documents `ids` of `collection` onto `node` from its peers.
pub(super) fn sync(collection: &str, ids: Vec<Text>, node: &str) -> Actions {
    SyncDocs {
        collection: collection.into(),
        doc_ids: ids,
        concurrency: 1,
        common: on(Some(node)),
        ..Default::default()
    }
    .into()
}

/// `node` shows `text`, the list `read_list` named `prefix`, and exactly the chunks of `text`.
fn shows(node: &str, text: &str, prefix: &str) -> Actions {
    let list: Vec<Value> = (0..=PARAGRAPHS)
        .map(|i| integration_test::m::r(&format!("{prefix}{i}")))
        .collect();
    let rows: Vec<Value> = texts(text)
        .into_iter()
        .map(|text| json!({ "text": text }))
        .collect();
    let mut common = on(Some(node));
    common.unordered = vec![format!("/data/{CHUNKS}")];
    eventually(
        Request {
            request: format!("query {{ Article {{ body _body_chunks }} {CHUNKS} {{ text }} }}")
                .into(),
            expected_data: json!({
                "Article": [{ "body": text, "_body_chunks": list }],
                CHUNKS: rows,
            }),
            common,
            ..Default::default()
        }
        .into(),
    )
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.concurrent")]
fn peers_that_edit_apart_end_with_the_chunks_of_the_merged_text() {
    let north = body(PARAGRAPHS, NORTH);
    let south = body(PARAGRAPHS, SOUTH);
    let mut actions = two_nodes(false);
    // Unsigned, the same create gives the same document on both nodes.
    actions.extend([
        add_article_on(&body(2, "start"), "article", Some("db0")),
        add_article_on(&body(2, "start"), "copy", Some("db1")),
        update_article_on("article", &north, Some("db0")),
        update_article_on("article", &south, Some("db1")),
        read_list("north", PARAGRAPHS, Some("db0")),
        connect(),
        sync("Article", vec![Text::reference("article")], "db0"),
        sync("Article", vec![Text::reference("article")], "db1"),
        shows("db0", &south, "north"),
        shows("db1", &south, "north"),
    ]);
    without_default_node(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.concurrent")]
fn a_peer_that_merges_a_newer_text_builds_its_chunks() {
    let north = body(PARAGRAPHS, NORTH);
    let mut actions = two_nodes(false);
    actions.extend([
        add_article_on(&body(2, "start"), "article", Some("db0")),
        add_article_on(&body(2, "start"), "copy", Some("db1")),
        update_article_on("article", &north, Some("db0")),
        read_list("north", PARAGRAPHS, Some("db0")),
        connect(),
        sync("Article", vec![Text::reference("article")], "db1"),
        shows("db1", &north, "north"),
    ]);
    without_default_node(actions).execute();
}
