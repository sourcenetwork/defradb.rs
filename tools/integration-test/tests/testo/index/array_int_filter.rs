//! An Int filter value finds the elements of an indexed Float array, as Go's
//! index does.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.inline_array.filter.any")]
fn an_int_value_matches_an_indexed_float_array() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Users { name: String  pageRatings: [Float] @index }".into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": "Shahzad", "pageRatings": [50, 80] })),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": "Fred", "pageRatings": [0, 60] })),
            ..Default::default()
        }
        .into(),
        Request {
            request: "query { Users(filter: {pageRatings: {_any: {_gt: 70}}}) { name } }".into(),
            expected_data: json!({ "Users": [{ "name": "Shahzad" }] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
