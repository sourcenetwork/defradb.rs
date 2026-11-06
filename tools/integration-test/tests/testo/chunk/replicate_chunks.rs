//! A peer gets the same chunk documents as the node that wrote them (`defra.chunk.replicate.chunks`).

use integration_test::actions::UpdateDoc;
use integration_test::scenario::{Common, Text};
use serde_json::json;

use super::support::{
    add_article_on, body, chunks_are_listed, eventually, list_is, read_list, replicated_pair,
    without_default_node, CHUNKS,
};

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.chunks")]
fn a_peer_has_the_same_chunk_documents() {
    let mut actions = replicated_pair(&["Article", CHUNKS]);
    actions.extend([
        add_article_on(&body(5, "rivers"), "article", Some("db0")),
        read_list("created", 5, Some("db0")),
        eventually(list_is("created", 5, Some("db1"))),
        eventually(chunks_are_listed("created", 5, Some("db1"))),
        UpdateDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            doc_map: json!({ "body": body(3, "mountains") }),
            common: Common {
                node: Some("db0".into()),
                retry: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        read_list("updated", 3, Some("db0")),
        eventually(list_is("updated", 3, Some("db1"))),
        eventually(chunks_are_listed("updated", 3, Some("db1"))),
    ]);
    without_default_node(actions).execute();
}
