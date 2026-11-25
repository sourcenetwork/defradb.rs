//! `_all` over an empty array is true, so an indexed array returns the
//! documents whose array is empty, as an unindexed one does. Go's index writes
//! no entry for an empty array and misses them, so Go does not run these.

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::m;
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

fn scenario(id: &'static str, sdl: &str, requests: Vec<Actions>) -> TestCase {
    let docs = [
        json!({ "name": "a", "tags": [1] }),
        json!({ "name": "b", "tags": [] }),
        json!({ "name": "c", "tags": [2] }),
        json!({ "name": "d", "tags": null }),
        json!({ "name": "e", "tags": [null] }),
        json!({ "name": "f", "tags": [1, null] }),
    ];
    let mut actions: Vec<Actions> = vec![AddCollection {
        sdl: sdl.into(),
        ..Default::default()
    }
    .into()];
    actions.extend(docs.into_iter().map(|doc| {
        AddDoc {
            collection: "User".into(),
            doc_map: Some(doc),
            ..Default::default()
        }
        .into()
    }));
    actions.extend(requests);
    TestCase {
        id,
        client_types: Some(CLIENTS),
        ..TestCase::from_actions(actions)
    }
}

#[test]
#[mantra_macros::req_test("defra.index.array.all-empty")]
fn all_on_an_indexed_array_finds_an_empty_array() {
    scenario(
        integration_test::scenario_id!(),
        "type User { name: String  tags: [Int] @index }",
        vec![
            request(
                "query { User(filter: {tags: {_all: {_eq: 1}}}, order: {name: ASC}) { name } }",
                json!({ "User": [{ "name": "a" }, { "name": "b" }] }),
            ),
            request(
                "query @explain(type: execute) { User(filter: {tags: {_all: {_eq: 1}}}) { name } }",
                json!(m::explain_scan(json!({ "indexFetches": 6 }))),
            ),
            request(
                "query { User(filter: {tags: {_all: {_in: [1, 2]}}}, order: {name: ASC}) { name } }",
                json!({ "User": [{ "name": "a" }, { "name": "b" }, { "name": "c" }] }),
            ),
        ],
    )
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.array.all-empty")]
fn all_with_a_range_on_an_indexed_array_finds_an_empty_array() {
    scenario(
        integration_test::scenario_id!(),
        "type User { name: String  tags: [Int] @index }",
        vec![request(
            "query { User(filter: {tags: {_all: {_gt: 0}}}, order: {name: ASC}) { name } }",
            json!({ "User": [{ "name": "a" }, { "name": "b" }, { "name": "c" }] }),
        )],
    )
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.array.all-empty")]
fn all_on_an_indexed_composite_array_finds_an_empty_array() {
    scenario(
        integration_test::scenario_id!(),
        r#"type User @index(includes: [{field: "tags"}, {field: "name"}]) {
            name: String
            tags: [Int]
        }"#,
        vec![request(
            "query { User(filter: {tags: {_all: {_eq: 1}}}, order: {name: ASC}) { name } }",
            json!({ "User": [{ "name": "a" }, { "name": "b" }] }),
        )],
    )
    .execute();
}
