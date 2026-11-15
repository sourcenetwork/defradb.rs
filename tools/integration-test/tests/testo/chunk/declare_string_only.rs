//! `@chunk` is refused off a String field (`defra.chunk.declare.string_only`).

use integration_test::actions::AddCollection;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("defra.chunk.declare.string_only")]
fn chunk_is_refused_off_a_string_field() {
    TestCase::from_actions(vec![AddCollection {
        sdl: "type Counter { n: Int @chunk }".into(),
        expected_error: Some(
            "@chunk is only allowed on a last-writer-wins String field. Field: Counter.n".into(),
        ),
        ..Default::default()
    }
    .into()])
    .execute();
}
