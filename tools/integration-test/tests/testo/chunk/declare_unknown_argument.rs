//! `@chunk` refuses an argument it does not take (`defra.chunk.declare.unknown_argument`).

use integration_test::actions::AddCollection;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("defra.chunk.declare.unknown_argument")]
fn chunk_is_refused_with_an_unknown_argument() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Note { body: String @chunk(size: 100) }".into(),
            expected_error: Some(
                "@chunk has no argument size. It takes maxChars and overlap. Field: Note.body"
                    .into(),
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
