//! A patch cannot rename a parent or its chunk collection (`defra.chunk.protect.rename`).

use integration_test::actions::{Actions, PatchCollection};
use integration_test::test_case::TestCase;

use super::support::{add_article_collection, CHUNKS};

const PARENT: &str = "Article";
const RENAMED: &str = "Story";
const REFUSED: &str = "Changed: the collection name";

fn rename(collection: &str) -> Actions {
    PatchCollection {
        patch: format!(
            r#"[{{ "op": "replace", "path": "/{collection}/Name", "value": "{RENAMED}" }}]"#
        )
        .into(),
        expected_error: Some(REFUSED.into()),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.chunk.protect.rename")]
fn renaming_the_parent_or_the_chunk_collection_is_refused() {
    TestCase::from_actions(vec![
        add_article_collection(),
        rename(PARENT),
        rename(CHUNKS),
    ])
    .execute();
}
