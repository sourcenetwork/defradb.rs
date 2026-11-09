use super::{around, capture_end, collection, doc, fails, names, only, query, unordered};
use integration_test::actions::Actions;
use integration_test::test_case::TestCase;
use serde_json::json;

/// `User` with an index on (age, score), `unique` or not, holding `users`.
fn users(unique: bool, users: &[(&str, i64, i64)]) -> Vec<Actions> {
    let mut actions = vec![collection(&format!(
        r#"type User @index(unique: {unique}, includes: [{{field: "age"}}, {{field: "score"}}]) {{ name: String age: Int score: Int }}"#
    ))];
    actions.extend(
        users
            .iter()
            .map(|(name, age, score)| doc(json!({ "name": name, "age": age, "score": score }))),
    );
    actions
}

const THREE: [(&str, i64, i64); 3] = [("alice", 20, 90), ("bob", 30, 80), ("carol", 40, 70)];

#[test]
#[mantra_macros::req_test("defra.cursor.composite_index")]
fn ordering_by_every_field_pages() {
    let mut actions = users(false, &THREE);
    actions.push(
        query(
            "query { _cursor { User(first: 2, order: [{age: ASC}, {score: ASC}]) { name age score } } }",
            only("User", json!([
                { "name": "alice", "age": 20, "score": 90 },
                { "name": "bob", "age": 30, "score": 80 },
            ])),
        )
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.composite_index")]
fn a_prefix_of_a_non_unique_index_fails() {
    let mut actions = users(false, &[("alice", 20, 90)]);
    actions.push(fails(
        "query { _cursor { User(first: 2, order: [{age: ASC}]) { name } } }",
        "no supporting index for cursor order field",
    ));
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.composite_index")]
fn a_prefix_of_a_unique_index_pages() {
    let mut actions = users(true, &[("alice", 20, 90), ("bob", 30, 80)]);
    actions.push(
        query(
            "query { _cursor { User(first: 2, order: [{age: ASC}]) { name } } }",
            only("User", unordered(names(&["alice", "bob"]))),
        )
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.composite_index")]
fn a_prefix_of_a_unique_index_returns_the_lowest_rows() {
    let mut actions = users(true, &THREE);
    actions.push(
        query(
            "query { _cursor { User(first: 2, order: [{age: ASC}]) { name } } }",
            only("User", unordered(names(&["alice", "bob"]))),
        )
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.composite_index")]
fn a_prefix_of_a_unique_index_resumes_after_a_token() {
    let mut actions = users(true, &THREE);
    actions.extend([
        capture_end(
            query(
                "query { _cursor { User(first: 1, order: [{age: ASC}]) { name age } _pageInfo { endCursor } } }",
                json!({ "_cursor": {
                    "User": [{ "name": "alice", "age": 20 }],
                    "_pageInfo": { "endCursor": { "$match": "nonemptyString" } },
                } }),
            ),
            "alice",
        ),
        query(
            around(
                "query { _cursor { User(first: 1, after: \"",
                "alice",
                "\", order: [{age: ASC}]) { name age } } }",
            ),
            only("User", json!([{ "name": "bob", "age": 30 }])),
        )
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}
