//! A field whose name starts with `_` can be chunked (`defra.chunk.declare.underscore_name`).

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.chunk.declare.underscore_name")]
fn a_field_whose_name_starts_with_an_underscore_is_chunked() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Note { _x: String @chunk }".into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Note".into(),
            doc_map: Some(json!({ "_x": "Some text." })),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Note___x_chunk { text } }".into(),
            expected_data: json!({ "Note___x_chunk": [{ "text": "Some text." }] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
