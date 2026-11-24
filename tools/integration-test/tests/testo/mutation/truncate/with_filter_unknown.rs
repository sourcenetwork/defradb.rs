//! `truncate_` checks the filter the request wrote, before its `docID` list is
//! folded in, so the error shows the filter as written. Texts measured on Go
//! DefraDB 307364d9d.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn truncate(declarations: &str, filter: &str) -> Text {
    Text::concat([
        Text::from(format!(
            "mutation{declarations} {{ truncate_User(docID: [\""
        )),
        Text::reference("alice"),
        Text::from(format!("\"], filter: {filter}) }}")),
    ])
}

fn run(
    id: &'static str,
    declarations: &str,
    filter: &str,
    variables: Value,
    data: Value,
    error: Option<&str>,
    left: Value,
) {
    TestCase {
        id,
        actions: vec![
            AddCollection {
                sdl: "type User { name: String }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    capture: captures([("alice", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Bob" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: truncate(declarations, filter),
                variables: Some(variables),
                expected_data: data,
                expected_error: error.map(Into::into),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { User { name } }".into(),
                expected_data: json!({ "User": left }),
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
fn truncate_with_doc_ids_and_an_unknown_literal_field_is_rejected() {
    run(
        integration_test::scenario_id!(),
        "",
        "{nope: {_eq: 1}}",
        json!({}),
        Value::Null,
        Some("Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field."),
        json!([{ "name": "Alice" }, { "name": "Bob" }]),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn truncate_with_doc_ids_and_an_unknown_variable_field_is_rejected() {
    run(
        integration_test::scenario_id!(),
        "($f: UserFilterArg)",
        "$f",
        json!({ "f": { "nope": { "_eq": 1 } } }),
        Value::Null,
        Some("Variable \"$f\" got invalid value {\"nope\":{\"_eq\":1}}.\nIn field \"nope\": Unknown field."),
        json!([{ "name": "Alice" }, { "name": "Bob" }]),
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn truncate_with_doc_ids_and_a_valid_nested_variable_runs() {
    run(
        integration_test::scenario_id!(),
        "($n: StringOperatorBlock)",
        "{name: $n}",
        json!({ "n": { "_eq": "Alice" } }),
        json!({ "truncate_User": true }),
        None,
        json!([{ "name": "Bob" }]),
    );
}
