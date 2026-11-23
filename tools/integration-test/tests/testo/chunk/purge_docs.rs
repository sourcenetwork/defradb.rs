//! Purging a parent removes its chunks and leaves other parents' (`defra.chunk.purge.docs`).

use integration_test::actions::PurgeDocs;
use integration_test::scenario::Text;
use integration_test::test_case::TestCase;

use super::support::{add_article, add_article_collection, body, chunk_texts_are};

#[test]
#[mantra_macros::req_test("defra.chunk.purge.docs")]
fn purging_a_parent_removes_only_its_chunks() {
    let (caves, lakes) = (body(3, "caves"), body(2, "lakes"));
    TestCase::from_actions(vec![
        add_article_collection(),
        add_article(&caves, "purged"),
        add_article(&lakes, "kept"),
        chunk_texts_are(&[&caves, &lakes], None),
        PurgeDocs {
            collection: "Article".into(),
            doc_ids: vec![Text::reference("purged")],
            expected_count: Some(1),
            ..Default::default()
        }
        .into(),
        chunk_texts_are(&[&lakes], None),
    ])
    .execute();
}
