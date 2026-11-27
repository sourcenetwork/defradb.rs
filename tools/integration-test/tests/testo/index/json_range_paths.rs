//! Range conditions on two paths of one JSON index each keep their own path.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn run(stored: &[Value], filter: &str, names: &[&str]) {
    let mut actions: Vec<Actions> = vec![AddCollection {
        sdl: "type User { name: String  custom: JSON @index }".into(),
        ..Default::default()
    }
    .into()];
    actions.extend(stored.iter().map(|doc| {
        Actions::from(AddDoc {
            collection: "User".into(),
            doc_map: Some(doc.clone()),
            ..Default::default()
        })
    }));
    let names: Vec<Value> = names.iter().map(|name| json!({ "name": name })).collect();
    actions.push(
        Request {
            request: format!("query {{ User(filter: {filter}, order: {{name: ASC}}) {{ name }} }}")
                .into(),
            expected_data: json!({ "User": names }),
            ..Default::default()
        }
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn ranges_on_two_paths() {
    run(
        &[
            json!({ "name": "Both", "custom": { "a": 30, "b": 5 } }),
            json!({ "name": "OnlyA", "custom": { "a": 30, "b": 25 } }),
            json!({ "name": "OnlyB", "custom": { "a": 1, "b": 5 } }),
        ],
        "{custom: {a: {_gt: 10}, b: {_lt: 20}}}",
        &["Both"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn all_ranges_on_two_paths() {
    run(
        &[
            json!({ "name": "Both", "custom": { "a": [30], "b": [5] } }),
            json!({ "name": "OnlyA", "custom": { "a": [30], "b": [25] } }),
            json!({ "name": "OnlyB", "custom": { "a": [1], "b": [5] } }),
        ],
        "{custom: {a: {_all: {_gt: 10}}, b: {_all: {_lt: 20}}}}",
        &["Both"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn a_range_and_ne_on_two_paths() {
    run(
        &[
            json!({ "name": "Both", "custom": { "a": 30, "b": 5 } }),
            json!({ "name": "Equal", "custom": { "a": 30, "b": 25 } }),
            json!({ "name": "Low", "custom": { "a": 1, "b": 5 } }),
        ],
        "{custom: {a: {_gt: 10}, b: {_ne: 25}}}",
        &["Both"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn two_bounds_on_one_path_still_narrow() {
    run(
        &[
            json!({ "name": "In", "custom": { "a": 15 } }),
            json!({ "name": "Low", "custom": { "a": 5 } }),
            json!({ "name": "High", "custom": { "a": 25 } }),
        ],
        "{custom: {a: {_gt: 10, _lt: 20}}}",
        &["In"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn like_and_ne_on_two_paths() {
    run(
        &[
            json!({ "name": "Both", "custom": { "a": "xa", "b": 5 } }),
            json!({ "name": "Equal", "custom": { "a": "xa", "b": 25 } }),
            json!({ "name": "Unlike", "custom": { "a": "ya", "b": 5 } }),
        ],
        "{custom: {a: {_like: \"x%\"}, b: {_ne: 25}}}",
        &["Both"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn a_range_and_like_on_two_paths() {
    run(
        &[
            json!({ "name": "Both", "custom": { "a": 15, "b": "xa" } }),
            json!({ "name": "Unlike", "custom": { "a": 15, "b": "ya" } }),
            json!({ "name": "Low", "custom": { "a": 1, "b": "xa" } }),
        ],
        "{custom: {a: {_gt: 10}, b: {_like: \"x%\"}}}",
        &["Both"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn like_and_a_range_on_two_paths() {
    run(
        &[
            json!({ "name": "Both", "custom": { "a": "xa", "b": 15 } }),
            json!({ "name": "Unlike", "custom": { "a": "ya", "b": 15 } }),
            json!({ "name": "Low", "custom": { "a": "xa", "b": 1 } }),
        ],
        "{custom: {a: {_like: \"x%\"}, b: {_gt: 10}}}",
        &["Both"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn eq_and_like_on_two_paths() {
    run(
        &[
            json!({ "name": "Both", "custom": { "a": 15, "b": "xa" } }),
            json!({ "name": "Unlike", "custom": { "a": 15, "b": "ya" } }),
            json!({ "name": "Other", "custom": { "a": 1, "b": "xa" } }),
        ],
        "{custom: {a: {_eq: 15}, b: {_like: \"x%\"}}}",
        &["Both"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn a_range_and_like_on_one_path() {
    run(
        &[
            json!({ "name": "Both", "custom": { "a": "xb" } }),
            json!({ "name": "Unlike", "custom": { "a": "yb" } }),
            json!({ "name": "Low", "custom": { "a": "xa" } }),
        ],
        "{custom: {a: {_gt: \"xa\", _like: \"x%\"}}}",
        &["Both"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn nin_and_like_with_a_missing_nin_path() {
    run(
        &[
            json!({ "name": "NoA", "custom": { "b": "xa" } }),
            json!({ "name": "NullA", "custom": { "a": null, "b": "xa" } }),
            json!({ "name": "Five", "custom": { "a": 5, "b": "xa" } }),
            json!({ "name": "Equal", "custom": { "a": 25, "b": "xa" } }),
            json!({ "name": "Unlike", "custom": { "a": 5, "b": "ya" } }),
        ],
        "{custom: {a: {_nin: [25]}, b: {_like: \"x%\"}}}",
        &["Five", "NoA", "NullA"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn ne_and_like_with_a_missing_ne_path() {
    run(
        &[
            json!({ "name": "NoA", "custom": { "b": "xa" } }),
            json!({ "name": "NullA", "custom": { "a": null, "b": "xa" } }),
            json!({ "name": "Five", "custom": { "a": 5, "b": "xa" } }),
            json!({ "name": "Equal", "custom": { "a": 25, "b": "xa" } }),
            json!({ "name": "Unlike", "custom": { "a": 5, "b": "ya" } }),
        ],
        "{custom: {a: {_ne: 25}, b: {_like: \"x%\"}}}",
        &["Five", "NullA"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn ranges_from_two_any_clauses_on_one_path() {
    run(
        &[
            json!({ "name": "Split", "custom": { "a": [5, 30] } }),
            json!({ "name": "In", "custom": { "a": [15] } }),
            json!({ "name": "Out", "custom": { "a": [5] } }),
        ],
        "{_and: [{custom: {a: {_any: {_gt: 10}}}}, {custom: {a: {_any: {_lt: 20}}}}]}",
        &["In", "Split"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json_range_paths")]
fn two_bounds_in_one_any_clause() {
    run(
        &[
            json!({ "name": "Split", "custom": { "a": [5, 30] } }),
            json!({ "name": "In", "custom": { "a": [15] } }),
            json!({ "name": "Out", "custom": { "a": [5] } }),
        ],
        "{custom: {a: {_any: {_gt: 10, _lt: 20}}}}",
        &["In"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn ne_under_all_on_two_paths_finds_the_empty_array() {
    run(
        &[
            json!({ "name": "EmptyB", "custom": { "a": [1], "b": [] } }),
            json!({ "name": "EmptyA", "custom": { "a": [], "b": [1] } }),
            json!({ "name": "Equal", "custom": { "a": [1], "b": [25] } }),
        ],
        "{custom: {a: {_all: {_ne: 25}}, b: {_all: {_ne: 25}}}}",
        &["EmptyA", "EmptyB"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.json.matches-scan")]
fn like_under_all_on_two_paths_finds_the_empty_array() {
    run(
        &[
            json!({ "name": "EmptyB", "custom": { "a": ["xa"], "b": [] } }),
            json!({ "name": "EmptyA", "custom": { "a": [], "b": ["xb"] } }),
            json!({ "name": "Other", "custom": { "a": ["xa"], "b": ["yb"] } }),
        ],
        "{custom: {a: {_all: {_like: \"x%\"}}, b: {_all: {_like: \"x%\"}}}}",
        &["EmptyA", "EmptyB"],
    );
}
