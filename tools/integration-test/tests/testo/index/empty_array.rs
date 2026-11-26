//! An empty array inside an indexed JSON value has an entry of its own at its
//! path: `_all` finds it, and no other filter takes it for a missing path or a
//! null. Typed arrays are covered by `all_empty_array.rs`.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn docs(collection: &str, docs: &[Value]) -> Vec<Actions> {
    docs.iter()
        .map(|doc| {
            AddDoc {
                collection: collection.into(),
                doc_map: Some(doc.clone()),
                ..Default::default()
            }
            .into()
        })
        .collect()
}

fn query(collection: &str, filter: &str, names: &[&str]) -> Actions {
    let names: Vec<Value> = names.iter().map(|name| json!({ "name": name })).collect();
    Request {
        request: format!(
            "query {{ {collection}(filter: {filter}, order: {{name: ASC}}) {{ name }} }}"
        )
        .into(),
        expected_data: json!({ collection: names }),
        ..Default::default()
    }
    .into()
}

fn run(sdl: &str, collection: &str, stored: &[Value], queries: &[(&str, &[&str])]) {
    let mut actions = vec![AddCollection {
        sdl: sdl.into(),
        ..Default::default()
    }
    .into()];
    actions.extend(docs(collection, stored));
    actions.extend(
        queries
            .iter()
            .map(|(filter, names)| query(collection, filter, names)),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn a_unique_json_index_holds_two_empty_arrays() {
    run(
        "type User { name: String  custom: JSON @index(unique: true) }",
        "User",
        &[
            json!({ "name": "A", "custom": { "tags": [] } }),
            json!({ "name": "B", "custom": { "tags": [] } }),
            json!({ "name": "C", "custom": { "tags": [3] } }),
        ],
        &[
            ("{custom: {tags: {_all: {_eq: 3}}}}", &["A", "B", "C"]),
            ("{custom: {tags: {_any: {_eq: 3}}}}", &["C"]),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn all_on_a_nested_json_array_finds_the_empty_array() {
    run(
        "type User { name: String  custom: JSON @index }",
        "User",
        &[
            json!({ "name": "Empty", "custom": { "tags": [], "b": 1 } }),
            json!({ "name": "NoTags", "custom": { "b": 1 } }),
            json!({ "name": "One", "custom": { "tags": [1] } }),
            json!({ "name": "Two", "custom": { "tags": [2] } }),
            json!({ "name": "Deep", "custom": { "tags": [[]] } }),
        ],
        &[
            ("{custom: {tags: {_all: {_eq: 1}}}}", &["Empty", "One"]),
            ("{custom: {tags: {_all: {_gt: 1}}}}", &["Empty", "Two"]),
            ("{custom: {tags: {_any: {_eq: 1}}}}", &["One"]),
            ("{custom: {tags: {_any: {_gt: 0}}}}", &["One", "Two"]),
            ("{custom: {b: {_eq: 1}}}", &["Empty", "NoTags"]),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn other_filters_on_a_top_level_json_array_skip_the_empty_array() {
    run(
        "type User { name: String  custom: JSON @index }",
        "User",
        &[
            json!({ "name": "Empty", "custom": [] }),
            json!({ "name": "Null", "custom": null }),
            json!({ "name": "One", "custom": [1] }),
        ],
        &[
            ("{custom: {_all: {_eq: 1}}}", &["Empty", "One"]),
            ("{custom: {_any: {_eq: 1}}}", &["One"]),
            ("{custom: {_any: {_gt: 0}}}", &["One"]),
            ("{custom: {_eq: null}}", &["Null"]),
        ],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn an_update_moves_the_empty_array_entry() {
    let mut actions = vec![AddCollection {
        sdl: "type User { name: String  custom: JSON @index }".into(),
        ..Default::default()
    }
    .into()];
    for (name, numbers) in [("Grows", json!([])), ("Shrinks", json!([7]))] {
        actions.push(
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": name, "custom": { "numbers": numbers } })),
                common: Common {
                    capture: captures([(name, "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(query(
        "User",
        "{custom: {numbers: {_all: {_eq: 5}}}}",
        &["Grows"],
    ));
    for (name, numbers) in [("Grows", json!([6])), ("Shrinks", json!([]))] {
        actions.push(
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference(name),
                doc_map: json!({ "custom": { "numbers": numbers } }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(query(
        "User",
        "{custom: {numbers: {_all: {_eq: 5}}}}",
        &["Shrinks"],
    ));
    actions.push(query(
        "User",
        "{custom: {numbers: {_any: {_eq: 6}}}}",
        &["Grows"],
    ));
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn ne_under_all_on_two_json_paths_finds_the_empty_array() {
    run(
        "type User { name: String  custom: JSON @index }",
        "User",
        &[
            json!({ "name": "EmptyB", "custom": { "a": [1], "b": [] } }),
            json!({ "name": "EmptyA", "custom": { "a": [], "b": [1] } }),
            json!({ "name": "Equal", "custom": { "a": [1], "b": [25] } }),
        ],
        &[(
            "{custom: {a: {_all: {_ne: 25}}, b: {_all: {_ne: 25}}}}",
            &["EmptyA", "EmptyB"],
        )],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.empty_array")]
fn like_under_all_on_two_json_paths_finds_the_empty_array() {
    run(
        "type User { name: String  custom: JSON @index }",
        "User",
        &[
            json!({ "name": "EmptyB", "custom": { "a": ["xa"], "b": [] } }),
            json!({ "name": "EmptyA", "custom": { "a": [], "b": ["xb"] } }),
            json!({ "name": "Other", "custom": { "a": ["xa"], "b": ["yb"] } }),
        ],
        &[(
            "{custom: {a: {_all: {_like: \"x%\"}}, b: {_all: {_like: \"x%\"}}}}",
            &["EmptyA", "EmptyB"],
        )],
    );
}
