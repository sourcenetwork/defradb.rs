//! Peers hold the same chunk documents, ID for ID, whatever order chunks and
//! edits reach them in (`defra.chunk.replicate.same_ids`).

use integration_test::actions::{Actions, Request};
use integration_test::m;
use integration_test::scenario::Text;
use serde_json::{json, Value};

use super::replicate_concurrent::{connect, sync, two_nodes};
use super::support::{
    add_article_on, body, eventually, on, replicated_pair, texts, update_article_on,
    without_default_node, CHUNKS,
};

/// `node` holds exactly the chunks of `text`, in text order, each `_docID`
/// named `<prefix><i>` so the other node's read must match it.
fn same_ids(node: &str, text: &str, prefix: &str) -> Actions {
    let mut chunks = texts(text);
    chunks.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    let rows: Vec<Value> = chunks
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| json!({ "_docID": m::same(&format!("{prefix}{i}")), "text": chunk }))
        .collect();
    eventually(
        Request {
            request: format!("query {{ {CHUNKS}(order: {{text: ASC}}) {{ _docID text }} }}").into(),
            expected_data: json!({ CHUNKS: rows }),
            common: on(Some(node)),
            ..Default::default()
        }
        .into(),
    )
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.same_ids")]
fn chunks_that_arrive_before_their_article_get_the_writers_ids() {
    let rivers = body(3, "rivers");
    let first = texts(&rivers).remove(0);
    let edited = format!("{}{}", first.as_str().unwrap(), body(2, "mountains"));
    // Only the chunks are pushed, so they reach db1 before the article does.
    let mut actions = replicated_pair(&[CHUNKS]);
    actions.extend([
        add_article_on(&rivers, "article", Some("db0")),
        update_article_on("article", &edited, Some("db0")),
        same_ids("db0", &edited, "edited"),
        sync("Article", vec![Text::reference("article")], "db1"),
        same_ids("db1", &edited, "edited"),
        update_article_on("article", &rivers, Some("db0")),
        same_ids("db0", &rivers, "back"),
        sync("Article", vec![Text::reference("article")], "db1"),
        same_ids("db1", &rivers, "back"),
    ]);
    without_default_node(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.same_ids")]
fn peers_that_edit_at_once_hold_the_same_chunk_ids() {
    let start = body(2, "start");
    let kept = texts(&start).remove(0);
    let north = body(3, "north");
    // Starts with the first paragraph, so that chunk is kept on db1 and made on db0.
    let south = format!("{}{}", kept.as_str().unwrap(), body(2, "south"));
    let mut actions = two_nodes(false);
    actions.extend([
        add_article_on(&start, "article", Some("db0")),
        add_article_on(&start, "copy", Some("db1")),
        update_article_on("article", &north, Some("db0")),
        update_article_on("article", &south, Some("db1")),
        connect(),
        sync("Article", vec![Text::reference("article")], "db0"),
        sync("Article", vec![Text::reference("article")], "db1"),
        eventually(
            Request {
                request: "query { Article { body } }".into(),
                expected_data: json!({ "Article": [{ "body": m::same("winner") }] }),
                common: on(Some("db0")),
                ..Default::default()
            }
            .into(),
        ),
        eventually(
            Request {
                request: "query { Article { body } }".into(),
                expected_data: json!({ "Article": [{ "body": m::same("winner") }] }),
                common: on(Some("db1")),
                ..Default::default()
            }
            .into(),
        ),
    ]);
    // "Paragraph 0 is about start" sorts after "Paragraph 0 is about north", so south wins.
    actions.extend([
        same_ids("db0", &south, "merged"),
        same_ids("db1", &south, "merged"),
    ]);
    without_default_node(actions).execute();
}
