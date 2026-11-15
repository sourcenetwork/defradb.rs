//! Re-adding a chunked type fails like a plain one (`defra.chunk.declare.already_exists`).

use integration_test::actions::AddCollection;
use integration_test::test_case::TestCase;

use super::support::{add_article_collection, SCHEMA};

#[test]
#[mantra_macros::req_test("defra.chunk.declare.already_exists")]
fn re_adding_a_chunked_type_reports_that_the_collection_already_exists() {
    TestCase::from_actions(vec![
        add_article_collection(),
        AddCollection {
            sdl: SCHEMA.into(),
            expected_error: Some("collection already exists. Name: Article".into()),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
