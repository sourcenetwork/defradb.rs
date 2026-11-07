//! `@chunk` refuses an argument given twice (`defra.chunk.declare.repeated_argument`).

use integration_test::actions::AddCollection;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("defra.chunk.declare.repeated_argument")]
fn chunk_is_refused_with_an_argument_given_twice() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Note { body: String @chunk(maxChars: 64, maxChars: 10) }".into(),
            expected_error: Some("@chunk has maxChars twice. Field: Note.body".into()),
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
