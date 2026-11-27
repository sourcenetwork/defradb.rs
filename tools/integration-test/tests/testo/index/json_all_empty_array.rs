//! `_all` over an empty array is true, so an indexed JSON field returns the
//! documents whose array at the filtered path is empty, as an unindexed one
//! does. The index's empty-array entry is seen by `_all` alone. Go's index
//! writes no entry for an empty array and misses them, so Go does not run these.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

const CLIENTS: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli];

fn request(query: &str, expected: Value) -> Actions {
    Request {
        request: query.into(),
        expected_data: expected,
        ..Default::default()
    }
    .into()
}

fn add(doc: Value) -> Actions {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(doc),
        ..Default::default()
    }
    .into()
}

fn names(names: &[&str]) -> Value {
    let names: Vec<Value> = names.iter().map(|name| json!({ "name": name })).collect();
    json!({ "User": names })
}

fn query(filter: &str) -> String {
    format!("query {{ User(filter: {{custom: {filter}}}, order: {{name: ASC}}) {{ name }} }}")
}

fn explain(filter: &str) -> String {
    format!("query @explain(type: execute) {{ User(filter: {{custom: {filter}}}) {{ name }} }}")
}

fn scenario(id: &'static str, sdl: &str, mut actions: Vec<Actions>) -> TestCase {
    let docs = [
        json!({ "name": "empty", "custom": { "a": [] } }),
        json!({ "name": "missing", "custom": {} }),
        json!({ "name": "nested empty", "custom": { "a": [[]] } }),
        json!({ "name": "null", "custom": { "a": null } }),
        json!({ "name": "one", "custom": { "a": [1] } }),
        json!({ "name": "one and null", "custom": { "a": [1, null] } }),
        json!({ "name": "scalar", "custom": { "a": 1 } }),
        json!({ "name": "two", "custom": { "a": [2] } }),
    ];
    let mut all: Vec<Actions> = vec![AddCollection {
        sdl: sdl.into(),
        ..Default::default()
    }
    .into()];
    all.extend(docs.into_iter().map(add));
    all.append(&mut actions);
    TestCase {
        id,
        client_types: Some(CLIENTS),
        ..TestCase::from_actions(all)
    }
}

const SDL: &str = "type User { name: String  custom: JSON @index }";

#[test]
#[mantra_macros::req_test("defra.index.json.all-empty")]
fn all_on_an_indexed_json_path_finds_an_empty_array() {
    scenario(
        integration_test::scenario_id!(),
        SDL,
        vec![
            request(&query("{a: {_all: {_eq: 1}}}"), names(&["empty", "one"])),
            request(
                &explain("{a: {_all: {_eq: 1}}}"),
                json!(m::explain_scan(json!({ "indexFetches": 3 }))),
            ),
            request(
                &query("{a: {_all: {_in: [1, 2]}}}"),
                names(&["empty", "one", "two"]),
            ),
            request(
                &query("{a: {_all: {_gt: 0}}}"),
                names(&["empty", "one", "two"]),
            ),
        ],
    )
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.json.all-empty")]
fn the_empty_array_entry_matches_no_other_filter() {
    scenario(
        integration_test::scenario_id!(),
        SDL,
        vec![
            request(
                &query("{a: {_any: {_eq: 1}}}"),
                names(&["one", "one and null"]),
            ),
            request(
                &query("{a: {_any: {_gt: 0}}}"),
                names(&["one", "one and null", "two"]),
            ),
            request(&query("{a: {_eq: 1}}"), names(&["scalar"])),
            request(
                &query("{a: {_none: {_eq: 1}}}"),
                names(&["empty", "nested empty", "two"]),
            ),
            request(&query("{a: {_any: {_eq: null}}}"), names(&["one and null"])),
            request(&query("{a: {_all: {_eq: null}}}"), names(&["empty"])),
        ],
    )
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.json.all-empty")]
fn all_on_a_descending_json_index_finds_an_empty_array() {
    scenario(
        integration_test::scenario_id!(),
        "type User { name: String  custom: JSON @index(direction: DESC) }",
        vec![
            request(&query("{a: {_all: {_eq: 1}}}"), names(&["empty", "one"])),
            request(
                &query("{a: {_all: {_gt: 0}}}"),
                names(&["empty", "one", "two"]),
            ),
            request(&query("{a: {_all: {_lt: 2}}}"), names(&["empty", "one"])),
        ],
    )
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.json.all-empty")]
fn all_on_two_paths_finds_an_empty_array_at_either_path() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(CLIENTS),
        ..TestCase::from_actions(vec![
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            add(json!({ "name": "a empty b filled", "custom": { "a": [], "b": [1] } })),
            add(json!({ "name": "a filled b empty", "custom": { "a": [1], "b": [] } })),
            request(
                &query("{a: {_all: {_gt: 0}}, b: {_all: {_eq: 1}}}"),
                names(&["a empty b filled", "a filled b empty"]),
            ),
        ])
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.json.all-empty")]
fn all_on_a_top_level_empty_array_finds_it() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(CLIENTS),
        ..TestCase::from_actions(vec![
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            add(json!({ "name": "empty", "custom": [] })),
            add(json!({ "name": "one", "custom": [1] })),
            request(&query("{_all: {_eq: 1}}"), names(&["empty", "one"])),
        ])
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.json.all-empty")]
fn all_on_a_composite_json_index_finds_an_empty_array() {
    scenario(
        integration_test::scenario_id!(),
        r#"type User @index(includes: [{field: "custom"}, {field: "name"}]) {
            name: String
            custom: JSON
        }"#,
        vec![request(
            &query("{a: {_all: {_eq: 1}}}"),
            names(&["empty", "one"]),
        )],
    )
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.json.all-empty")]
fn empty_arrays_never_collide_in_a_unique_json_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(CLIENTS),
        ..TestCase::from_actions(vec![
            AddCollection {
                sdl: "type User { name: String  custom: JSON @index(unique: true) }".into(),
                ..Default::default()
            }
            .into(),
            add(json!({ "name": "first", "custom": { "a": [] } })),
            add(json!({ "name": "second", "custom": { "a": [] } })),
            request(&query("{a: {_all: {_eq: 1}}}"), names(&["first", "second"])),
        ])
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.json.all-empty")]
fn an_update_moves_the_empty_array_entry() {
    TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(CLIENTS),
        ..TestCase::from_actions(vec![
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "filled", "custom": { "a": [1] } })),
                common: Common {
                    capture: captures([("filled", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "emptied", "custom": { "a": [] } })),
                common: Common {
                    capture: captures([("emptied", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            request(&query("{a: {_all: {_eq: 2}}}"), names(&["emptied"])),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("filled"),
                doc_map: json!({ "custom": { "a": [] } }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("emptied"),
                doc_map: json!({ "custom": { "a": [3] } }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            request(&query("{a: {_all: {_eq: 2}}}"), names(&["filled"])),
        ])
    }
    .execute();
}
