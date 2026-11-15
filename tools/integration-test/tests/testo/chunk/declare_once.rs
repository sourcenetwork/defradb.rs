//! `@chunk` may appear once on a field (`defra.chunk.declare.once`).

use integration_test::actions::AddCollection;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("defra.chunk.declare.once")]
fn a_second_chunk_on_one_field_is_refused() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Note { body: String @chunk(maxChars: 64) @chunk(maxChars: 100) }".into(),
            expected_error: Some(
                "@chunk can only be used once on a field. Field: Note.body".into(),
            ),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: "type Note { body: String }".into(),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
