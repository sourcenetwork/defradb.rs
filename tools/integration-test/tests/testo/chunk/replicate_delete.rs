//! A parent delete replicates with its chunks, even when no chunk delete
//! reaches the peer (`defra.chunk.replicate.delete`).

use integration_test::actions::{DeleteDoc, Request};
use integration_test::scenario::{Common, Text};
use serde_json::json;

use super::replicate_concurrent::{connect, sync, two_nodes};
use super::support::{
    add_article_on, body, chunks_are_listed, eventually, no_chunks, on, read_chunks,
    replicated_pair, without_default_node, CHUNKS,
};

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.delete")]
fn a_parent_delete_replicates_with_its_chunks() {
    let mut actions = replicated_pair(&["Article", CHUNKS]);
    actions.extend([
        add_article_on(&body(4, "lakes"), "article", Some("db0")),
        read_chunks("created", 4, Some("db0")),
        eventually(chunks_are_listed("created", 4, Some("db1"))),
        DeleteDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            common: Common {
                node: Some("db0".into()),
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        eventually(
            Request {
                request: "query { Article { _docID } }".into(),
                expected_data: json!({ "Article": [] }),
                common: on(Some("db1")),
                ..Default::default()
            }
            .into(),
        ),
        eventually(no_chunks(Some("db1"))),
    ]);
    without_default_node(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.delete")]
fn a_merged_delete_removes_the_chunks_a_peer_made_itself() {
    let text = body(3, "lakes");
    let mut actions = two_nodes(false);
    actions.extend([
        add_article_on(&text, "article", Some("db0")),
        add_article_on(&text, "copy", Some("db1")),
        read_chunks("held", 3, Some("db1")),
        DeleteDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            common: Common {
                node: Some("db0".into()),
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        // The chunk deletes went out while the nodes were apart, so only the article's arrives.
        connect(),
        sync("Article", vec![Text::reference("article")], "db1"),
        eventually(
            Request {
                request: "query { Article { _docID } }".into(),
                expected_data: json!({ "Article": [] }),
                common: on(Some("db1")),
                ..Default::default()
            }
            .into(),
        ),
        eventually(no_chunks(Some("db1"))),
    ]);
    without_default_node(actions).execute();
}
