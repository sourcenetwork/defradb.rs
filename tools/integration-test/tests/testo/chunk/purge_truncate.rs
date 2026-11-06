//! Truncating the parent empties its chunks (`defra.chunk.purge.truncate`).

use integration_test::actions::Truncate;
use integration_test::test_case::TestCase;

use super::support::{
    add_article, add_article_collection, body, chunks_are_listed, no_chunks, read_list,
};

#[test]
#[mantra_macros::req_test("defra.chunk.purge.truncate")]
fn truncating_the_parent_empties_its_chunks() {
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&body(4, "caves"), "article"),
        read_list("created", 4, None),
        chunks_are_listed("created", 4, None),
        Truncate {
            collection: "Article".into(),
            ..Default::default()
        }
        .into(),
        no_chunks(None),
    ])
    .execute();
}
