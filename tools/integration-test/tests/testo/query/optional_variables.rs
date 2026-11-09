//! An optional variable the request leaves out drops the input field it
//! fills, as Go does (#2104, `defra.request.optional_variables`). The parser
//! test `test_unset_optional_variable_omits_its_field` proves the drop; this
//! proves the request it leaves behind, an empty field condition, filters on
//! nothing.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

fn book(title: &str, plot: &str, rating: f64) -> Actions {
    AddDoc {
        collection: "Book".into(),
        doc_map: Some(json!({ "title": title, "plot": plot, "rating": rating })),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.request.optional_variables")]
fn an_empty_field_condition_filters_on_nothing() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Book { title: String plot: String rating: Float64 }".into(),
            ..Default::default()
        }
        .into(),
        book("1984", "Big Brother is watching", 4.7),
        book("Lord of the Flies", "Boys on an island", 3.9),
        Request {
            request: "query { Book(filter: {plot: {}, rating: {_geq: 4.2}}) { title } }".into(),
            expected_data: json!({ "Book": [{ "title": "1984" }] }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
