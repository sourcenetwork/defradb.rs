//! `_in` on the first field of a composite index: unordered, the lookups run
//! as the filter lists them; ordered by that field, they follow the order.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const DESC_FIRST: &str = r#"
    type User @index(includes: [{field: "name", direction: DESC}, {field: "age", direction: ASC}]) {
        name: String
        age: Int
    }
"#;

const ASC_FIRST: &str = r#"
    type User @index(includes: [{field: "name", direction: ASC}, {field: "age", direction: DESC}]) {
        name: String
        age: Int
    }
"#;

fn users_then(id: &'static str, sdl: &str, query: &str, expected: Value) {
    let mut actions = vec![AddCollection {
        sdl: sdl.into(),
        ..Default::default()
    }
    .into()];
    for (name, age) in [
        ("Shahzad", 20),
        ("Bruno", 23),
        ("Fred", 28),
        ("Andy", 33),
        ("Addo", 42),
        ("Chris", 55),
    ] {
        actions.push(
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": name, "age": age })),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        Request {
            request: query.into(),
            expected_data: expected,
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        id,
        actions,
        ..Default::default()
    }
    .execute();
}

fn names(names: [&str; 3]) -> Value {
    json!({ "User": names.map(|name| json!({ "name": name })) })
}

#[test]
#[mantra_macros::req_test("go.index.order.composite-in")]
fn in_on_descending_first_field_without_order_keeps_the_written_order() {
    users_then(
        integration_test::scenario_id!(),
        DESC_FIRST,
        r#"query { User(filter: {name: {_in: ["Fred", "Addo", "Andy"]}}) { name } }"#,
        names(["Fred", "Addo", "Andy"]),
    );
}

#[test]
#[mantra_macros::req_test("go.index.order.composite-in")]
fn in_on_ascending_first_field_without_order_keeps_the_written_order() {
    users_then(
        integration_test::scenario_id!(),
        ASC_FIRST,
        r#"query { User(filter: {name: {_in: ["Fred", "Addo", "Andy"]}}) { name } }"#,
        names(["Fred", "Addo", "Andy"]),
    );
}

#[test]
#[mantra_macros::req_test("go.index.order.composite-in")]
fn in_on_descending_first_field_ordered_descending_follows_the_order() {
    users_then(
        integration_test::scenario_id!(),
        DESC_FIRST,
        r#"query { User(filter: {name: {_in: ["Addo", "Andy", "Fred"]}}, order: {name: DESC}) { name } }"#,
        names(["Fred", "Andy", "Addo"]),
    );
}

#[test]
#[mantra_macros::req_test("go.index.order.composite-in")]
fn in_on_descending_first_field_ordered_ascending_follows_the_order() {
    users_then(
        integration_test::scenario_id!(),
        DESC_FIRST,
        r#"query { User(filter: {name: {_in: ["Fred", "Addo", "Andy"]}}, order: {name: ASC}) { name } }"#,
        names(["Addo", "Andy", "Fred"]),
    );
}
