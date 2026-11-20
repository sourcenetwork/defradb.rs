//! `_any` and `_all` on an indexed top-level JSON field look only at array
//! elements, never at a scalar of the same value, as Go's index does.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn case(docs: &[Value], filter: &str, names: &[&str]) {
    let mut actions = vec![AddCollection {
        sdl: "type Users { name: String  custom: JSON @index }".into(),
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
#[mantra_macros::req_test("go.query.json.array-ops")]
fn any_skips_a_scalar_of_the_same_value() {
    case(
        &[
            json!({ "name": "Keenan", "custom": 3 }),
            json!({ "name": "Bruno", "custom": [null, 3] }),
        ],
        "{_any: {_eq: 3}}",
        &["Bruno"],
    );
}

#[test]
#[mantra_macros::req_test("go.query.json.array-ops")]
fn all_skips_a_scalar_of_the_same_value() {
    case(
        &[
            json!({ "name": "Shahzad", "custom": [1] }),
            json!({ "name": "Keenan", "custom": 1 }),
            json!({ "name": "Andy", "custom": [1, "1"] }),
        ],
        "{_all: {_eq: 1}}",
        &["Shahzad"],
    );
}

#[test]
#[mantra_macros::req_test("go.query.json.array-ops")]
fn all_keeps_an_empty_array() {
    case(
        &[
            json!({ "name": "Empty", "custom": [] }),
            json!({ "name": "One", "custom": [1] }),
            json!({ "name": "Two", "custom": [2] }),
        ],
        "{_all: {_eq: 1}}",
        &["Empty", "One"],
    );
}
