//! Both bounds of a range filter limit the index scan (Go #5341,
//! `defra.index.range_filter`).

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

/// The ten users of Go's index suite (`getUserDocs`), by age.
const USERS: [(&str, i64); 10] = [
    ("Shahzad", 20),
    ("Bruno", 23),
    ("Fred", 28),
    ("John", 30),
    ("Islam", 32),
    ("Andy", 33),
    ("Addo", 42),
    ("Roy", 44),
    ("Keenan", 48),
    ("Chris", 55),
];

fn range_case(index: &str, filter: &str, names: &[&str], fetches: u64) {
    let mut actions = vec![AddCollection {
        sdl: format!("type User {{ name: String  age: Int {index} }}").into(),
        ..Default::default()
    }
    .into()];
    actions.extend(USERS.iter().map(|(name, age)| {
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": name, "age": age })),
            ..Default::default()
        }
        .into()
    }));
    let query = format!("User(filter: {{age: {filter}}}) {{ name }}");
    let names: Vec<Value> = names.iter().map(|name| json!({ "name": name })).collect();
    actions.push(
        Request {
            request: format!("query {{ {query} }}").into(),
            expected_data: json!({ "User": names }),
            ..Default::default()
        }
        .into(),
    );
    actions.push(
        Request {
            request: format!("query @explain(type: execute) {{ {query} }}").into(),
            expected_data: json!(m::explain_scan(json!({ "indexFetches": fetches }))),
            ..Default::default()
        }
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.range_filter")]
fn a_closed_range_fetches_only_the_range() {
    range_case(
        "@index",
        "{_geq: 28, _leq: 33}",
        &["Fred", "John", "Islam", "Andy"],
        4,
    );
}

#[test]
#[mantra_macros::req_test("defra.index.range_filter")]
fn an_open_range_on_a_descending_index_fetches_only_the_range() {
    range_case(
        "@index(direction: DESC)",
        "{_gt: 28, _lt: 44}",
        &["Addo", "Andy", "Islam", "John"],
        4,
    );
}

#[test]
#[mantra_macros::req_test("defra.index.range_filter")]
fn an_empty_range_fetches_nothing() {
    range_case("@index", "{_gt: 44, _lt: 30}", &[], 0);
}
