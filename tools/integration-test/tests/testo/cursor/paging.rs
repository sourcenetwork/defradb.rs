use super::{around, capture_end, collection, doc, names, only, query, same, unordered, USERS};
use integration_test::actions::Actions;
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const UNINDEXED: &str = "type User { name: String age: Int }";

fn users(sdl: &str, users: &[(&str, i64)]) -> Vec<Actions> {
    let mut actions = vec![collection(sdl)];
    actions.extend(
        users
            .iter()
            .map(|(name, age)| doc(json!({ "name": name, "age": age }))),
    );
    actions
}

/// Five `User`s with only a name, so `_docID` alone orders them.
fn named_only() -> Vec<Actions> {
    let mut actions = vec![collection(UNINDEXED)];
    actions.extend(["a", "b", "c", "d", "e"].map(|name| doc(json!({ "name": name }))));
    actions
}

fn end_cursor(has_next: Option<bool>) -> Value {
    let mut info = json!({ "endCursor": { "$match": "nonemptyString" } });
    if let Some(has_next) = has_next {
        info["hasNext"] = json!(has_next);
    }
    info
}

#[test]
#[mantra_macros::req_test("defra.cursor.paging")]
fn last_returns_the_final_rows() {
    let mut actions = users(
        USERS,
        &[("alice", 20), ("bob", 30), ("carol", 40), ("dave", 50)],
    );
    actions.push(
        query(
            "query { _cursor { User(last: 2, order: [{age: ASC}]) { name } } }",
            only("User", unordered(names(&["carol", "dave"]))),
        )
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.paging")]
fn no_order_pages_by_doc_id_without_an_index() {
    let mut actions = users(UNINDEXED, &[("alpha", 1), ("beta", 2)]);
    actions.push(
        query(
            "query { _cursor { User(first: 2) { name } } }",
            only("User", unordered(names(&["alpha", "beta"]))),
        )
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.paging")]
fn before_excludes_its_boundary() {
    let mut actions = users(USERS, &[("a", 20), ("b", 30), ("c", 40), ("d", 50)]);
    actions.extend([
        capture_end(
            query(
                "query { _cursor { User(first: 3, order: [{age: ASC}]) { name age } _pageInfo { endCursor } } }",
                json!({ "_cursor": {
                    "User": [{ "name": "a", "age": 20 }, { "name": "b", "age": 30 }, { "name": "c", "age": 40 }],
                    "_pageInfo": end_cursor(None),
                } }),
            ),
            "c",
        ),
        query(
            around(
                "query { _cursor { User(last: 2, before: \"",
                "c",
                "\", order: [{age: ASC}]) { name age } } }",
            ),
            only("User", json!([{ "name": "a", "age": 20 }, { "name": "b", "age": 30 }])),
        )
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.paging")]
fn pages_through_duplicate_index_keys() {
    let names = ["alice", "bob", "carol", "dave"];
    let mut actions = users(
        "type User { name: String age: Int @index score: Int }",
        &names.map(|name| (name, 30)),
    );
    let one_of_them = json!({ "name": { "$match": "all", "of": [
        { "$match": "unique", "name": "seen" },
        { "$match": "anyOf", "of": names },
    ] } });
    let pair = json!([one_of_them, one_of_them]);
    actions.extend([
        capture_end(
            query(
                "query { _cursor { User(first: 2, order: [{age: ASC}]) { name } _pageInfo { endCursor hasNext } } }",
                json!({ "_cursor": { "User": pair, "_pageInfo": end_cursor(Some(true)) } }),
            ),
            "second",
        ),
        query(
            around(
                "query { _cursor { User(first: 2, after: \"",
                "second",
                "\", order: [{age: ASC}]) { name } } }",
            ),
            only("User", pair),
        )
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.paging")]
fn first_after_follows_doc_id_desc() {
    let mut actions = named_only();
    actions.extend([
        query(
            "query { User(order: {_docID: DESC}) { name } }",
            json!({ "User": same("name", &["d0", "d1", "d2", "d3", "d4"]) }),
        )
        .into(),
        capture_end(
            query(
                "query { _cursor { User(first: 2, order: [{_docID: DESC}]) { name } _pageInfo { endCursor hasNext } } }",
                json!({ "_cursor": {
                    "User": same("name", &["d0", "d1"]),
                    "_pageInfo": end_cursor(Some(true)),
                } }),
            ),
            "d1",
        ),
        query(
            around(
                "query { _cursor { User(first: 2, after: \"",
                "d1",
                "\", order: [{_docID: DESC}]) { name } } }",
            ),
            only("User", same("name", &["d2", "d3"])),
        )
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.paging")]
fn last_before_follows_doc_id_desc() {
    let mut actions = named_only();
    actions.extend([
        query(
            "query { User(order: {_docID: DESC}) { name } }",
            json!({ "User": same("name", &["d0", "d1", "d2", "d3", "d4"]) }),
        )
        .into(),
        capture_end(
            query(
                "query { _cursor { User(first: 3, order: [{_docID: DESC}]) { name } _pageInfo { endCursor } } }",
                json!({ "_cursor": {
                    "User": same("name", &["d0", "d1", "d2"]),
                    "_pageInfo": end_cursor(None),
                } }),
            ),
            "d2",
        ),
        query(
            around(
                "query { _cursor { User(last: 2, before: \"",
                "d2",
                "\", order: [{_docID: DESC}]) { name } } }",
            ),
            only("User", unordered(same("name", &["d0", "d1"]))),
        )
        .into(),
    ]);
    TestCase::from_actions(actions).execute();
}
