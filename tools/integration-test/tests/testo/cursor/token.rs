use super::{aged, page, rows, USERS};
use integration_test::scenario::Text;
use integration_test::test_case::TestCase;
use serde_json::json;

/// `{"d":"bae-6064fa74-3ca9-5fc5-b08d-2bfcd839488d","k":{"age":25}}`, Alice's row.
const ALICE_25: &str =
    "eyJkIjoiYmFlLTYwNjRmYTc0LTNjYTktNWZjNS1iMDhkLTJiZmNkODM5NDg4ZCIsImsiOnsiYWdlIjoyNX19";
/// `{"d":"bae-b99540cd-c9aa-5b6a-9080-a97f918c1186","k":{"age":30}}`, Bob's row.
const BOB_30: &str =
    "eyJkIjoiYmFlLWI5OTU0MGNkLWM5YWEtNWI2YS05MDgwLWE5N2Y5MThjMTE4NiIsImsiOnsiYWdlIjozMH19";
/// Alice's row as older builds wrote it: `{"d":…,"k":{"age":25},"o":""}`.
const ALICE_25_WITH_O: &str = "eyJkIjoiYmFlLTYwNjRmYTc0LTNjYTktNWZjNS1iMDhkLTJiZmNkODM5NDg4ZCIsImsiOnsiYWdlIjoyNX0sIm8iOiIifQ";

fn alice_bob_carol() -> Vec<integration_test::actions::Actions> {
    let mut actions = aged(USERS, &[]);
    actions.extend(
        [("Alice", 25), ("Bob", 30), ("Carol", 35)]
            .map(|(name, age)| super::doc(json!({ "name": name, "age": age }))),
    );
    actions
}

#[test]
#[mantra_macros::req_test("defra.cursor.token_encoding")]
fn a_token_is_its_rows_doc_id_and_order_values() {
    let mut actions = alice_bob_carol();
    actions.push(
        page(
            "first: 2, order: {age: ASC}",
            None,
            "name",
            json!({ "_cursor": {
                "User": [{ "name": "Alice" }, { "name": "Bob" }],
                "_pageInfo": {
                    "hasNext": true,
                    "hasPrev": false,
                    "startCursor": ALICE_25,
                    "endCursor": BOB_30,
                },
            } }),
        )
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.cursor.token_encoding")]
fn a_token_with_an_empty_o_still_resumes() {
    let mut actions = alice_bob_carol();
    for token in [ALICE_25, ALICE_25_WITH_O] {
        actions.push(
            page(
                "first: 2, order: {age: ASC}",
                Some(Text::from(token)),
                "name",
                rows(json!([{ "name": "Bob" }, { "name": "Carol" }]), false, true),
            )
            .into(),
        );
    }
    TestCase::from_actions(actions).execute();
}
