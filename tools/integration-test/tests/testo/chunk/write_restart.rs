//! The chunks of a parent read back the same after the node reopens its store (`defra.chunk.write.restart`).

use integration_test::actions::{DeleteDoc, NewNode, Restart};
use integration_test::scenario::Text;
use integration_test::test_case::TestCase;

use super::support::{
    add_article, add_article_collection, body, chunk_texts_are, chunks_are_listed, no_chunks,
    read_chunks, update_article,
};

#[test]
#[mantra_macros::req_test("defra.chunk.write.restart")]
fn chunks_read_back_after_the_node_restarts() {
    let (rivers, hills) = (body(5, "rivers"), body(3, "hills"));
    TestCase::from_actions(vec![
        NewNode {
            database_dir: Some("".into()),
            ..Default::default()
        }
        .into(),
        add_article_collection(),
        add_article(&rivers, "article"),
        read_chunks("river", 5, None),
        Restart::default().into(),
        chunks_are_listed("river", 5, None),
        chunk_texts_are(&[&rivers], None),
        update_article("article", &hills),
        chunk_texts_are(&[&hills], None),
        Restart::default().into(),
        chunk_texts_are(&[&hills], None),
        DeleteDoc {
            collection: "Article".into(),
            doc_id: Text::reference("article"),
            ..Default::default()
        }
        .into(),
        Restart::default().into(),
        no_chunks(None),
    ])
    .execute();
}
