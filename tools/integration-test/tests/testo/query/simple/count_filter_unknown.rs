//! An aggregate selector's `filter` is checked against its input type, written
//! inline or supplied as the whole selector by a variable. Texts measured on
//! Go DefraDB 307364d9d.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const SDL: &str = r#"
    type Author { name: String age: Int books: [Book] }
    type Book { title: String rating: Float author: Author }
"#;

fn run(id: &'static str, request: &str, variables: Value, data: Value, error: Option<&str>) {
    TestCase {
        id,
        actions: vec![
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "A" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: request.into(),
                variables: Some(variables),
                expected_data: data,
                expected_error: error.map(Into::into),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_field_in_an_inline_count_filter_is_rejected() {
    run(
        integration_test::scenario_id!(),
        "query { Author { name COUNT(books: {filter: {nope: {_eq: 1}}, limit: 2}) } }",
        json!({}),
        Value::Null,
        Some("Argument \"books\" has invalid value {filter: {nope: {_eq: 1}}, limit: 2}.\nIn field \"filter\": In field \"nope\": Unknown field."),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_field_in_a_count_selector_variable_is_rejected() {
    run(
        integration_test::scenario_id!(),
        "query($t: Book__CountSelector) { Author { name COUNT(books: $t) } }",
        json!({ "t": { "filter": { "nope": { "_eq": 1 } }, "limit": 1 } }),
        Value::Null,
        Some("Variable \"$t\" got invalid value {\"filter\":{\"nope\":{\"_eq\":1}},\"limit\":1}.\nIn field \"filter\": In field \"nope\": Unknown field."),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_count_selector_variable_with_a_list_filter_is_rejected() {
    run(
        integration_test::scenario_id!(),
        "query($t: Book__CountSelector) { Author { name COUNT(books: $t) } }",
        json!({ "t": { "filter": [{ "nope": { "_eq": 1 } }] } }),
        Value::Null,
        Some("Variable \"$t\" got invalid value {\"filter\":[{\"nope\":{\"_eq\":1}}]}.\nIn field \"filter\": Expected \"BookFilterArg\", found not an object."),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_valid_count_selector_variable_still_counts() {
    run(
        integration_test::scenario_id!(),
        "query($t: Book__CountSelector) { Author { name COUNT(books: $t) } }",
        json!({ "t": { "filter": { "title": { "_eq": "x" } } } }),
        json!({ "Author": [{ "name": "A", "COUNT": 0 }] }),
        None,
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn unknown_field_in_a_group_count_filter_is_rejected() {
    run(
        integration_test::scenario_id!(),
        "query { Author(groupBy: [name]) { name COUNT(GROUP: {filter: {nope: {_eq: 1}}}) } }",
        json!({}),
        Value::Null,
        Some("Argument \"GROUP\" has invalid value {filter: {nope: {_eq: 1}}}.\nIn field \"filter\": In field \"nope\": Unknown field."),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_valid_group_count_filter_still_counts() {
    run(
        integration_test::scenario_id!(),
        "query { Author(groupBy: [name]) { name COUNT(GROUP: {filter: {name: {_eq: \"A\"}}}) } }",
        json!({}),
        json!({ "Author": [{ "name": "A", "COUNT": 1 }] }),
        None,
    );
}
