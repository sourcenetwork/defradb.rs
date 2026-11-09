//! `@chunk` is refused in a view, and the view is not stored (`defra.chunk.declare.no_view`).

use integration_test::actions::{AddCollection, AddView};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("defra.chunk.declare.no_view")]
fn chunk_is_refused_in_a_view_and_nothing_is_stored() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Source { title: String body: String }".into(),
            ..Default::default()
        }
        .into(),
        AddView {
            query: "Source { title body }".into(),
            sdl: "type SourceView { title: String body: String @chunk(maxChars: 64) }".into(),
            expected_error: Some("@chunk is not allowed in a view. Field: SourceView.body".into()),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: "type SourceView { title: String } type SourceView__body_chunk { text: String }"
                .into(),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
