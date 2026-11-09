use super::{doc, names, query, USERS};
use integration_test::actions::Actions;
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn users(users: &[(&str, i64)]) -> Vec<Actions> {
    let mut actions = vec![super::collection(USERS)];
    actions.extend(
        users
            .iter()
            .map(|(name, age)| doc(json!({ "name": name, "age": age }))),
    );
    actions
}

fn token() -> Value {
    json!({ "$match": "nonemptyString" })
}

#[test]
#[mantra_macros::req_test("defra.cursor.page_info")]
fn only_the_selected_fields_appear() {
    let mut actions = users(&[("alice", 20), ("bob", 30), ("carol", 40)]);
    actions.push(
        query(
            "query { _cursor { User(first: 2, order: [{age: ASC}]) { name } _pageInfo { hasNext startCursor endCursor } } }",
            json!({ "_cursor": {
                "User": names(&["alice", "bob"]),
                "_pageInfo": { "hasNext": true, "startCursor": token(), "endCursor": token() },
            } }),
        )
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.page_info")]
fn field_aliases_are_ignored() {
    let mut actions = users(&[("a", 20), ("b", 30)]);
    actions.push(
        query(
            "query { _cursor { User(first: 1, order: [{age: ASC}]) { name } _pageInfo { next: hasNext start: startCursor } } }",
            json!({ "_cursor": {
                "User": names(&["a"]),
                "_pageInfo": { "hasNext": true, "startCursor": token() },
            } }),
        )
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.page_info")]
fn a_block_alias_is_ignored() {
    let mut actions = users(&[("a", 20), ("b", 30)]);
    actions.push(
        query(
            "query { _cursor { User(first: 1, order: [{age: ASC}]) { name } info: _pageInfo { hasNext } } }",
            json!({ "_cursor": { "User": names(&["a"]), "_pageInfo": { "hasNext": true } } }),
        )
        .into(),
    );
    TestCase::from_actions(actions).execute();
}
