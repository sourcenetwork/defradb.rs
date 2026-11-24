//! A parent delete replicates with its chunks (`defra.chunk.replicate.delete`).

use integration_test::actions::{DeleteDoc, Request};
use integration_test::scenario::{Common, Text};
use serde_json::json;

use super::support::{
    add_article_on, body, chunks_are_listed, eventually, no_chunks, on, read_list, replicated_pair,
    without_default_node, CHUNKS,
};

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.delete")]
fn a_parent_delete_replicates_with_its_chunks() {
    let mut actions = replicated_pair(&["Article", CHUNKS]);
    actions.extend([
        add_article_on(&body(4, "lakes"), "article", Some("db0")),
        read_list("created", 4, Some("db0")),
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
        no_chunks(Some("db1")),
    ]);
    without_default_node(actions).execute();
}
