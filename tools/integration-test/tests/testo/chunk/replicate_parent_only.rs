//! A replicator that names only the parent also ships its chunks (`defra.chunk.replicate.parent_only`).

use super::support::{
    add_article_on, body, chunks_are_listed, eventually, list_is, read_list, replicated_pair,
    without_default_node,
};

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.parent_only")]
fn a_replicator_of_the_parent_ships_its_chunks() {
    let mut actions = replicated_pair(&["Article"]);
    actions.extend([
        add_article_on(&body(4, "valleys"), "article", Some("db0")),
        read_list("created", 4, Some("db0")),
        eventually(list_is("created", 4, Some("db1"))),
        eventually(chunks_are_listed("created", 4, Some("db1"))),
    ]);
    without_default_node(actions).execute();
}
