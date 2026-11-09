use super::{collection, doc, fails, USERS};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.cursor.errors")]
fn an_order_field_without_an_index_fails() {
    TestCase::from_actions(vec![
        collection("type User { name: String age: Int }"),
        doc(json!({ "name": "a", "age": 1 })),
        fails(
            "query { _cursor { User(first: 1, order: [{age: ASC}]) { name } } }",
            "no supporting index for cursor order field",
        ),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.errors")]
fn a_token_that_is_not_base64_fails() {
    TestCase::from_actions(vec![
        collection(USERS),
        doc(json!({ "name": "a", "age": 1 })),
        fails(
            r#"query { _cursor { User(first: 1, after: "!!!not-base64!!!", order: [{age: ASC}]) { name } } }"#,
            "invalid cursor",
        ),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.errors")]
fn first_and_last_together_fail() {
    TestCase::from_actions(vec![
        collection(USERS),
        fails(
            "query { _cursor { User(first: 5, last: 3, order: [{age: ASC}]) { name } } }",
            "forward parameters (first/after) cannot be combined with backward parameters (last/before)",
        ),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.errors")]
fn two_collections_in_one_block_fail() {
    TestCase::from_actions(vec![
        collection(USERS),
        collection("type Book { title: String }"),
        fails(
            "query { _cursor { User(first: 1, order: [{age: ASC}]) { name } Book(first: 1) { title } } }",
            "_cursor block cannot contain multiple collection queries",
        ),
    ])
    .execute();
}
