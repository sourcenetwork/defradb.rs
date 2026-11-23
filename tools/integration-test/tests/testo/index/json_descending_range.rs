//! An open range on a descending JSON index finds the values past its bound.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn case(docs: &[Value], filter: &str, names: &[&str]) {
    let mut actions = vec![AddCollection {
        sdl: "type Users { name: String  custom: JSON @index(direction: DESC) }".into(),
        ..Default::default()
    }
    .into()];
    actions.extend(docs.iter().map(|doc| {
        AddDoc {
            collection: "Users".into(),
            doc_map: Some(doc.clone()),
            ..Default::default()
        }
        .into()
    }));
    let names: Vec<Value> = names.iter().map(|name| json!({ "name": name })).collect();
    actions.push(
        Request {
            request: format!("query {{ Users(filter: {{custom: {filter}}}) {{ name }} }}").into(),
            expected_data: json!({ "Users": names }),
            ..Default::default()
        }
        .into(),
    );
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.index.range_filter")]
fn a_lower_bound_on_a_path() {
    case(
        &[
            json!({ "name": "Low", "custom": { "a": 1 } }),
            json!({ "name": "High", "custom": { "a": 5 } }),
        ],
        "{a: {_gt: 3}}",
        &["High"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.range_filter")]
fn an_upper_bound_on_a_path() {
    case(
        &[
            json!({ "name": "Low", "custom": { "a": 1 } }),
            json!({ "name": "High", "custom": { "a": 5 } }),
        ],
        "{a: {_lt: 3}}",
        &["Low"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.range_filter")]
fn a_lower_bound_on_a_top_level_number() {
    case(
        &[
            json!({ "name": "Low", "custom": 3 }),
            json!({ "name": "High", "custom": 5 }),
        ],
        "{_gt: 4}",
        &["High"],
    );
}

#[test]
#[mantra_macros::req_test("defra.index.range_filter")]
fn an_equality_on_a_top_level_bool() {
    case(
        &[
            json!({ "name": "False", "custom": false }),
            json!({ "name": "True", "custom": true }),
        ],
        "{_eq: true}",
        &["True"],
    );
}
