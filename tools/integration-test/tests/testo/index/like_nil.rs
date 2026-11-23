//! A document without the indexed string passes `_nlike` and never `_like`,
//! as it does without the index.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn case(filter: &str, names: Value) {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Users { name: String @index  age: Int }".into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "name": "Viserys I Targaryen, King of the Andals", "age": 1 })),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(json!({ "age": 2 })),
            ..Default::default()
        }
        .into(),
        Request {
            request: format!("query {{ Users(filter: {{name: {filter}}}) {{ age }} }}").into(),
            expected_data: json!({ "Users": names }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.filter.pattern.nlike")]
fn nlike_returns_a_document_without_the_field() {
    case(r#"{_nlike: "%King%"}"#, json!([{ "age": 2 }]));
}

#[test]
#[mantra_macros::req_test("go.query.simple.filter.pattern.like")]
fn like_skips_a_document_without_the_field() {
    case(r#"{_like: "%"}"#, json!([{ "age": 1 }]));
}
