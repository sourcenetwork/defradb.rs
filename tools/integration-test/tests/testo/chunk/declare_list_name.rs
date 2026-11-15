//! `@chunk` is refused where its list field name would be invalid
//! (`defra.chunk.declare.list_name`).

use integration_test::actions::AddCollection;
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("defra.chunk.declare.list_name")]
fn chunk_is_refused_on_a_field_whose_list_name_would_start_with_two_underscores() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Note { _x: String @chunk }".into(),
            expected_error: Some(
                "@chunk is not allowed on a field whose name starts with \"_\", because its list field __x_chunks would start with \"__\". Field: Note._x"
                    .into(),
            ),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: "type Note { _x: String }".into(),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
