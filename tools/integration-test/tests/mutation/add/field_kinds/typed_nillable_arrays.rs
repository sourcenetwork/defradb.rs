//! Lists given for nillable-element array fields are stored whole, every
//! element in order (Go shinzo-develop 9015737fe,
//! `go.shinzo.document.typed_nillable_arrays`; before it Go dropped a typed
//! Go slice silently).

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.shinzo.document.typed_nillable_arrays")]
fn string_and_datetime_lists_fill_nillable_element_arrays() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Block { cids: [String]  times: [DateTime] }".into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Block".into(),
            doc_map: Some(json!({
                "cids": ["bafyA", "bafyB", "bafyC"],
                "times": ["2026-07-07T12:00:00Z", "2026-07-08T12:00:00Z"],
            })),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Block { cids times } }".into(),
            expected_data: json!({ "Block": [{
                "cids": ["bafyA", "bafyB", "bafyC"],
                "times": ["2026-07-07T12:00:00Z", "2026-07-08T12:00:00Z"],
            }] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
