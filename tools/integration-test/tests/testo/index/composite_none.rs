//! `_none` on the second field of a composite index returns the same documents
//! as the same query without the index (sourcenetwork/defradb#3401).

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

const INDEXED_SDL: &str = r#"
    type User @index(includes: [{field: "age"}, {field: "numbers"}]) {
        name: String
        age: Int
        numbers: [Int!]
    }
"#;

const PLAIN_SDL: &str = r#"
    type User {
        name: String
        age: Int
        numbers: [Int!]
    }
"#;

const FIRST_AND_NONE: &str = r#"
    query {
        User(filter: {age: {_eq: 30}, numbers: {_none: {_eq: 2}}}) {
            name
        }
    }
"#;

const FIRST_AND_NONE_EXPLAIN: &str = r#"
    query @explain(type: execute) {
        User(filter: {age: {_eq: 30}, numbers: {_none: {_eq: 2}}}) {
            name
        }
    }
"#;

const ONLY_NONE_EXPLAIN: &str = r#"
    query @explain(type: execute) {
        User(filter: {numbers: {_none: {_eq: 2}}}) {
            name
        }
    }
"#;

const ONLY_NONE: &str = r#"
    query {
        User(filter: {numbers: {_none: {_eq: 2}}}) {
            name
        }
    }
"#;

fn add_user(name: &str, age: i64, numbers: &[i64]) -> AddDoc {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": name, "age": age, "numbers": numbers })),
        ..Default::default()
    }
}

fn unordered_users() -> Common {
    Common {
        unordered_documents: vec!["/data/User".into()],
        ..Default::default()
    }
}

fn scenario(id: &'static str, sdl: &str, requests: Vec<Request>) -> TestCase {
    let mut actions = vec![
        AddCollection {
            sdl: sdl.into(),
            ..Default::default()
        }
        .into(),
        add_user("John", 30, &[1, 2]).into(),
        add_user("Shahzad", 30, &[3, 4]).into(),
        add_user("Andy", 30, &[2, 5]).into(),
        add_user("Fred", 40, &[5]).into(),
    ];
    actions.extend(requests.into_iter().map(Into::into));
    TestCase {
        id,
        actions,
        ..Default::default()
    }
}

fn first_and_none() -> Request {
    Request {
        request: FIRST_AND_NONE.into(),
        expected_data: json!({ "User": [{ "name": "Shahzad" }] }),
        ..Default::default()
    }
}

fn only_none() -> Request {
    Request {
        request: ONLY_NONE.into(),
        expected_data: json!({ "User": [{ "name": "Shahzad" }, { "name": "Fred" }] }),
        common: unordered_users(),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.index.query.composite-none")]
fn composite_index_with_none_on_second_field_uses_first_field() {
    scenario(
        integration_test::scenario_id!(),
        INDEXED_SDL,
        vec![
            first_and_none(),
            Request {
                request: FIRST_AND_NONE_EXPLAIN.into(),
                expected_data: json!(m::explain_scan(
                    json!({ "indexFetches": 6, "filterMatches": 1 })
                )),
                ..Default::default()
            },
        ],
    )
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.query.composite-none")]
fn none_on_second_field_without_index_matches() {
    scenario(
        integration_test::scenario_id!(),
        PLAIN_SDL,
        vec![first_and_none()],
    )
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.query.composite-none")]
fn composite_index_with_only_none_on_second_field_scans() {
    scenario(
        integration_test::scenario_id!(),
        INDEXED_SDL,
        vec![
            only_none(),
            Request {
                request: ONLY_NONE_EXPLAIN.into(),
                expected_data: json!(m::explain_scan(
                    json!({ "indexFetches": 0, "filterMatches": 2 })
                )),
                ..Default::default()
            },
        ],
    )
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.query.composite-none")]
fn only_none_without_index_matches() {
    scenario(
        integration_test::scenario_id!(),
        PLAIN_SDL,
        vec![only_none()],
    )
    .execute();
}
