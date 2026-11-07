use super::{around, capture_end, collection, doc, only, query, same};
use integration_test::actions::Actions;
use integration_test::test_case::TestCase;
use serde_json::json;

/// `User` with one index to filter on (score) and one to order by (age).
fn users(users: &[(&str, i64, i64)]) -> Vec<Actions> {
    let mut actions = vec![collection(
        "type User { name: String score: Int @index age: Int @index }",
    )];
    actions.extend(
        users
            .iter()
            .map(|(name, age, score)| doc(json!({ "name": name, "age": age, "score": score }))),
    );
    actions
}

fn end(has_next: bool) -> serde_json::Value {
    json!({ "endCursor": { "$match": "nonemptyString" }, "hasNext": has_next })
}

#[test]
#[mantra_macros::req_test("defra.cursor.filter_index")]
fn a_filter_index_and_an_order_index_page_consistently() {
    let mut actions = users(&[
        ("a", 20, 90),
        ("b", 30, 90),
        ("c", 40, 90),
        ("d", 50, 80),
        ("e", 60, 90),
    ]);
    let row = |name: &str, age: i64| json!({ "name": name, "age": age, "score": 90 });
    actions.extend([
        capture_end(
            query(
                "query { _cursor { User(first: 2, filter: {score: {_eq: 90}}, order: [{age: ASC}]) { name age score } _pageInfo { endCursor hasNext } } }",
                json!({ "_cursor": { "User": [row("a", 20), row("b", 30)], "_pageInfo": end(true) } }),
            ),
            "b",
        ),
        query(
            around(
                "query { _cursor { User(first: 2, after: \"",
                "b",
                "\", filter: {score: {_eq: 90}}, order: [{age: ASC}]) { name age score } } }",
            ),
            only("User", json!([row("c", 40), row("e", 60)])),
        )
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.filter_index")]
fn order_ties_keep_their_order_across_a_page_boundary() {
    let mut actions = users(&[
        ("a", 40, 80),
        ("b", 50, 80),
        ("c", 50, 85),
        ("d", 50, 90),
        ("e", 50, 95),
        ("f", 60, 100),
    ]);
    let args = "filter: {score: {_gte: 80}}, order: [{age: ASC}]";
    actions.extend([
        query(
            format!("query {{ _cursor {{ User(first: 6, {args}) {{ name }} }} }}"),
            only("User", same("name", &["t0", "t1", "t2", "t3", "t4", "t5"])),
        )
        .into(),
        capture_end(
            query(
                format!("query {{ _cursor {{ User(first: 3, {args}) {{ name }} _pageInfo {{ endCursor hasNext }} }} }}"),
                json!({ "_cursor": { "User": same("name", &["t0", "t1", "t2"]), "_pageInfo": end(true) } }),
            ),
            "t2",
        ),
        query(
            around(
                "query { _cursor { User(first: 3, after: \"",
                "t2",
                &format!("\", {args}) {{ name }} }} }}"),
            ),
            only("User", same("name", &["t3", "t4", "t5"])),
        )
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}
